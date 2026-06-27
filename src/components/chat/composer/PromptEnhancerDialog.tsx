import {useEffect} from 'react';
import {createPortal} from 'react-dom';
import {useTranslation} from 'react-i18next';
import {Loader2, Sparkles, X} from 'lucide-react';
import {isEditableShortcutTarget, isEnterShortcutControl,} from '../../../utils/dialogShortcuts';

type PromptEnhancerShortcutAction = 'use-enhanced' | 'close' | null;

interface PromptEnhancerShortcutState {
    isLoading: boolean;
    hasEnhancedPrompt: boolean;
}

interface PromptEnhancerDialogProps {
    isOpen: boolean;
    isLoading: boolean;
    originalPrompt: string;
    enhancedPrompt: string;
    onUseEnhanced: () => void;
    onKeepOriginal: () => void;
    onClose: () => void;
}

type PromptEnhancerDialogLabel =
    | 'title'
    | 'close'
    | 'original'
    | 'enhanced'
    | 'loading'
    | 'keepOriginal'
    | 'useEnhanced';

const PROMPT_ENHANCER_DIALOG_LABELS: Record<PromptEnhancerDialogLabel, { key: string; fallback: string }> = {
    title: {
        key: 'chat.enhancer.title',
        fallback: 'Enhance prompt',
    },
    close: {
        key: 'common.close',
        fallback: 'Close',
    },
    original: {
        key: 'chat.enhancer.original',
        fallback: 'Original prompt',
    },
    enhanced: {
        key: 'chat.enhancer.enhanced',
        fallback: 'Enhanced prompt',
    },
    loading: {
        key: 'chat.enhancer.loading',
        fallback: 'Enhancing prompt...',
    },
    keepOriginal: {
        key: 'chat.enhancer.keepOriginal',
        fallback: 'Keep original',
    },
    useEnhanced: {
        key: 'chat.enhancer.useEnhanced',
        fallback: 'Use enhanced',
    },
};

export function getPromptEnhancerDialogLabels(
    translate: (key: string) => string,
): Record<PromptEnhancerDialogLabel, string> {
    return Object.fromEntries(
        Object.entries(PROMPT_ENHANCER_DIALOG_LABELS).map(([name, label]) => {
            const translated = translate(label.key);
            return [name, translated && translated !== label.key ? translated : label.fallback];
        }),
    ) as Record<PromptEnhancerDialogLabel, string>;
}

export function resolvePromptEnhancerShortcutAction(
    key: string,
    target: EventTarget | null,
    {
        isLoading,
        hasEnhancedPrompt,
    }: PromptEnhancerShortcutState,
): PromptEnhancerShortcutAction {
    if (key === 'Escape') {
        return isEditableShortcutTarget(target) ? null : 'close';
    }
    if (key === 'Enter') {
        if (isLoading || !hasEnhancedPrompt || isEnterShortcutControl(target)) return null;
        return 'use-enhanced';
    }
    return null;
}

/**
 * Prompt 增强结果对比弹窗：左原文 / 右增强版，可采用或保留。
 * 自包含 portal 弹窗（ModalDialog 固定页脚，无法满足自定义按钮）。
 */
export function PromptEnhancerDialog({
    isOpen,
    isLoading,
    originalPrompt,
    enhancedPrompt,
    onUseEnhanced,
    onKeepOriginal,
    onClose,
}: PromptEnhancerDialogProps) {
    const { t } = useTranslation();
    const labels = getPromptEnhancerDialogLabels(t);

    useEffect(() => {
        if (!isOpen) return;
        const onKeyDown = (event: KeyboardEvent) => {
            const action = resolvePromptEnhancerShortcutAction(event.key, event.target, {
                isLoading,
                hasEnhancedPrompt: enhancedPrompt.length > 0,
            });
            if (!action) return;
            event.preventDefault();
            if (action === 'use-enhanced') {
                onUseEnhanced();
            } else {
                onClose();
            }
        };
        window.addEventListener('keydown', onKeyDown);
        return () => window.removeEventListener('keydown', onKeyDown);
    }, [enhancedPrompt, isLoading, isOpen, onClose, onUseEnhanced]);

    if (!isOpen) return null;

    return createPortal(
        <div className="modal modal-open z-[100]">
            <div data-tauri-drag-region className="fixed top-0 left-0 right-0 h-8 z-[110]" />
            <div className="modal-box relative max-w-3xl bg-white dark:bg-base-100 shadow-2xl rounded-2xl max-h-[85vh] flex flex-col">
                <div className="flex items-center justify-between mb-4">
                    <h3 className="text-lg font-bold flex items-center gap-2">
                        <Sparkles size={18} className="text-primary" />
                        {labels.title}
                    </h3>
                    <button
                        className="btn btn-ghost btn-sm btn-circle"
                        onClick={onClose}
                        title={labels.close}
                    >
                        <X size={16} />
                    </button>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-3 flex-1 overflow-hidden">
                    <div className="flex flex-col min-h-0">
                        <div className="text-xs font-medium text-base-content/50 mb-1">
                            {labels.original}
                        </div>
                        <div className="flex-1 rounded-lg border border-base-300 bg-base-200/50 p-3 text-sm whitespace-pre-wrap overflow-y-auto">
                            {originalPrompt}
                        </div>
                    </div>
                    <div className="flex flex-col min-h-0">
                        <div className="text-xs font-medium text-primary mb-1">
                            {labels.enhanced}
                        </div>
                        <div className="flex-1 rounded-lg border border-primary/40 bg-primary/5 p-3 text-sm whitespace-pre-wrap overflow-y-auto">
                            {isLoading ? (
                                <span className="flex items-center gap-2 text-base-content/50">
                                    <Loader2 size={14} className="animate-spin" />
                                    {labels.loading}
                                </span>
                            ) : (
                                enhancedPrompt
                            )}
                        </div>
                    </div>
                </div>

                <div className="flex justify-end gap-2 mt-4">
                    <button className="btn btn-ghost btn-sm" onClick={onKeepOriginal}>
                        {labels.keepOriginal}
                    </button>
                    <button
                        className="btn btn-primary btn-sm"
                        onClick={onUseEnhanced}
                        disabled={isLoading || !enhancedPrompt}
                    >
                        {labels.useEnhanced}
                    </button>
                </div>
            </div>
            <div
                className="modal-backdrop bg-black/40 backdrop-blur-sm fixed inset-0 z-[-1]"
                onClick={onClose}
            />
        </div>,
        document.body,
    );
}
