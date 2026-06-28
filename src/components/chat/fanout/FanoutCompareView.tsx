import {useMemo, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {GitMerge, Trash2} from 'lucide-react';
import {useChatStore, type ChatSessionTab} from '../../../stores/useChatStore';
import {canMergeFanoutTab} from './compare';

interface FanoutCompareViewProps {
    tabs: ChatSessionTab[];
}

function latestAssistantText(tab: ChatSessionTab): string {
    for (let index = tab.messages.length - 1; index >= 0; index -= 1) {
        const message = tab.messages[index];
        if (message.role === 'assistant') {
            return message.content.trim();
        }
    }
    return '';
}

export function FanoutCompareView({tabs}: FanoutCompareViewProps) {
    const {t} = useTranslation();
    const focusTab = useChatStore((state) => state.focusTab);
    const discardFanoutAgent = useChatStore((state) => state.discardFanoutAgent);
    const mergeFanoutWinner = useChatStore((state) => state.mergeFanoutWinner);
    const [busyKey, setBusyKey] = useState<string | null>(null);

    const sortedTabs = useMemo(() => (
        [...tabs].sort((left, right) => left.createdAt - right.createdAt)
    ), [tabs]);

    const handleDiscard = async (tab: ChatSessionTab) => {
        if (!window.confirm(t('chat.fanout.confirmDiscard', 'Discard this fan-out agent and remove its worktree?'))) {
            return;
        }
        setBusyKey(tab.key);
        try {
            await discardFanoutAgent(tab.key);
        } finally {
            setBusyKey(null);
        }
    };

    const handleMerge = async (tab: ChatSessionTab) => {
        if (!window.confirm(t('chat.fanout.confirmMerge', 'Merge this fan-out branch into the current repository?'))) {
            return;
        }
        setBusyKey(tab.key);
        try {
            const outcome = await mergeFanoutWinner(tab.key);
            if (outcome === 'conflict') {
                window.alert(t('chat.fanout.mergeConflict', 'Merge conflict detected. The merge was aborted.'));
            }
        } finally {
            setBusyKey(null);
        }
    };

    if (sortedTabs.length <= 1) return null;

    return (
        <div className="mb-3 rounded-md border border-base-300 bg-base-100/85 p-2">
            <div className="mb-2 flex items-center justify-between gap-2">
                <span className="text-xs font-semibold text-base-content/70">
                    {t('chat.fanout.compare', 'Fan-out compare')}
                </span>
                <span className="text-[11px] text-base-content/45">
                    {t('chat.fanout.agentCount', '{{count}} agents', {count: sortedTabs.length})}
                </span>
            </div>
            <div className="grid gap-2 lg:grid-cols-2 xl:grid-cols-3">
                {sortedTabs.map((tab) => {
                    const preview = latestAssistantText(tab);
                    const busy = busyKey === tab.key;
                    return (
                        <article key={tab.key} className="min-w-0 rounded-md border border-base-300 bg-base-200/30 p-2">
                            <button
                                type="button"
                                className="mb-1 block max-w-full truncate text-left text-xs font-semibold text-primary hover:underline"
                                onClick={() => focusTab(tab.key)}
                            >
                                {tab.provider} · {tab.model}
                            </button>
                            <div className="mb-2 flex min-h-16 max-h-32 overflow-y-auto rounded-md bg-base-100/70 p-2 text-xs leading-5 text-base-content/75">
                                {preview || t('chat.fanout.waiting', 'Waiting for output...')}
                            </div>
                            <div className="flex items-center justify-end gap-1">
                                <button
                                    type="button"
                                    className="btn btn-ghost btn-xs"
                                    disabled={busy}
                                    onClick={() => void handleDiscard(tab)}
                                >
                                    <Trash2 size={13} aria-hidden />
                                    <span>{t('chat.fanout.discard', 'Discard')}</span>
                                </button>
                                <button
                                    type="button"
                                    className="btn btn-primary btn-xs"
                                    disabled={busy || !canMergeFanoutTab(tab)}
                                    onClick={() => void handleMerge(tab)}
                                >
                                    <GitMerge size={13} aria-hidden />
                                    <span>{t('chat.fanout.merge', 'Merge')}</span>
                                </button>
                            </div>
                        </article>
                    );
                })}
            </div>
        </div>
    );
}
