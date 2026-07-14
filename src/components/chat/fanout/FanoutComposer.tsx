import {useEffect, useMemo, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {GitBranchPlus} from 'lucide-react';
import {useChatStore} from '../../../stores/useChatStore';
import {buildFanoutPlan, type FanoutPick} from '../../../stores/fanoutPlan';
import {useProviderStore} from '../../../stores/useProviderStore';
import {rosterPicksFromProviders} from './roster';

interface FanoutComposerProps {
    prompt: string;
    repoRoot?: string;
    disabled?: boolean;
    onLaunched?: () => void;
}

export function FanoutComposer({
    prompt,
    repoRoot,
    disabled = false,
    onLaunched,
}: FanoutComposerProps) {
    const {t} = useTranslation();
    const {providers, hasLoaded, loadAllProviders} = useProviderStore();
    const launchFanout = useChatStore((state) => state.launchFanout);
    const [selected, setSelected] = useState<Record<string, boolean>>({});
    const [models, setModels] = useState<Record<string, string>>({});
    const [launching, setLaunching] = useState(false);

    useEffect(() => {
        void loadAllProviders();
    }, [loadAllProviders]);

    const roster = useMemo(() => rosterPicksFromProviders(providers), [providers]);

    useEffect(() => {
        if (!hasLoaded) return;
        setSelected((current) => {
            if (Object.keys(current).length > 0) return current;
            return Object.fromEntries(roster.slice(0, 2).map((pick) => [pick.providerId, true]));
        });
        setModels((current) => ({
            ...Object.fromEntries(roster.map((pick) => [pick.providerId, pick.models[0]?.id ?? ''])),
            ...current,
        }));
    }, [hasLoaded, roster]);

    const picks: FanoutPick[] = roster.flatMap((pick) => {
        const model = models[pick.providerId]?.trim();
        if (!selected[pick.providerId] || !model) return [];
        return [{
            providerId: pick.providerId,
            chatProvider: pick.chatProvider,
            model,
        }];
    });

    const canLaunch = Boolean(repoRoot?.trim()) && prompt.trim().length > 0 && picks.length > 0 && !disabled && !launching;

    const handleLaunch = async () => {
        const normalizedRepo = repoRoot?.trim();
        const normalizedPrompt = prompt.trim();
        if (!normalizedRepo || !normalizedPrompt || picks.length === 0) return;
        setLaunching(true);
        try {
            await launchFanout(normalizedRepo, buildFanoutPlan(normalizedPrompt, picks));
            onLaunched?.();
        } finally {
            setLaunching(false);
        }
    };

    return (
        <div className="mx-1 mb-2 rounded-md border border-base-300 bg-base-100/80 p-2">
            <div className="mb-2 flex items-center justify-between gap-2">
                <span className="text-xs font-medium text-base-content/70">
                    {t('chat.fanout.roster', 'Fan-out roster')}
                </span>
                <button
                    type="button"
                    className="btn btn-primary btn-xs gap-1"
                    disabled={!canLaunch}
                    onClick={handleLaunch}
                >
                    <GitBranchPlus size={14} aria-hidden />
                    <span>{launching ? t('chat.fanout.launching', 'Launching') : t('chat.fanout.launch', 'Launch')}</span>
                </button>
            </div>
            <div className="grid gap-2 sm:grid-cols-2">
                {roster.map((pick) => (
                    <label
                        key={pick.providerId}
                        className="flex min-w-0 items-center gap-2 rounded-md border border-base-300 bg-base-200/40 px-2 py-1.5"
                    >
                        <input
                            type="checkbox"
                            className="checkbox checkbox-primary checkbox-xs"
                            checked={Boolean(selected[pick.providerId])}
                            onChange={(event) => setSelected((current) => ({
                                ...current,
                                [pick.providerId]: event.target.checked,
                            }))}
                        />
                        <span className="min-w-0 flex-1 truncate text-xs">
                            {pick.providerName}
                        </span>
                        <select
                            className="select select-bordered select-xs max-w-[11rem]"
                            value={models[pick.providerId] ?? pick.models[0]?.id ?? ''}
                            onChange={(event) => setModels((current) => ({
                                ...current,
                                [pick.providerId]: event.target.value,
                            }))}
                        >
                            {pick.models.map((model) => (
                                <option key={model.id} value={model.id}>{model.label}</option>
                            ))}
                        </select>
                    </label>
                ))}
            </div>
        </div>
    );
}
