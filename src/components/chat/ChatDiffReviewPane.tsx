import {forwardRef, useEffect, useRef, useState} from 'react';
import {
    Check,
    Columns2,
    Copy,
    ExternalLink,
    FileDiff,
    PanelRightClose,
    Rows3,
    ScrollText,
    TextWrap
} from 'lucide-react';
import {useTranslation} from 'react-i18next';
import {copyToClipboard, openFile} from '../../utils/bridge';
import type {ChatStatusEditSummary} from '../../utils/chatStatusSummary';
import {cn} from '../../utils/cn';
import EditDiffPreview, {type EditDiffPreviewMode} from '../toolBlocks/EditDiffPreview';

interface ChatDiffReviewPaneProps {
    edit?: ChatStatusEditSummary;
    mode: EditDiffPreviewMode;
    wrapLines: boolean;
    currentCwd?: string | null;
    onModeChange: (mode: EditDiffPreviewMode) => void;
    onWrapLinesChange: (wrapLines: boolean) => void;
    onCollapse?: () => void;
}

const ChatDiffReviewPane = forwardRef<HTMLElement, ChatDiffReviewPaneProps>(function ChatDiffReviewPane({
    edit,
    mode,
    wrapLines,
    currentCwd,
    onModeChange,
    onWrapLinesChange,
    onCollapse,
}, ref) {
    const { t } = useTranslation();
    const [copied, setCopied] = useState(false);
    const copyTimerRef = useRef<number | null>(null);

    useEffect(() => () => {
        if (copyTimerRef.current !== null) {
            window.clearTimeout(copyTimerRef.current);
        }
    }, []);

    const handleOpenFile = () => {
        if (!edit) return;
        void openFile(edit.openPath, edit.lineStart, edit.lineEnd, currentCwd);
    };

    const handleCopyPath = async () => {
        if (!edit) return;
        await copyToClipboard(edit.openPath || edit.displayPath);
        setCopied(true);
        if (copyTimerRef.current !== null) {
            window.clearTimeout(copyTimerRef.current);
        }
        copyTimerRef.current = window.setTimeout(() => {
            setCopied(false);
            copyTimerRef.current = null;
        }, 2000);
    };

    const diffModeButtonClass = (viewMode: EditDiffPreviewMode) => cn(
        'status-diff-mode-button',
        mode === viewMode && 'active',
    );
    const translateWithFallback = (key: string, fallback: string, options?: Record<string, unknown>) => {
        const translated = options ? t(key, options) : t(key);
        return translated === key ? fallback : translated;
    };
    const diffPanelBaseLabel = translateWithFallback('chat.layout.diffPanel', 'File diff');
    const diffPanelLabel = edit
        ? translateWithFallback(
            'chat.layout.diffPanelForFile',
            `File diff: ${edit.displayPath}`,
            {file: edit.displayPath},
        )
        : diffPanelBaseLabel;
    const diffPanelEmptyLabel = translateWithFallback(
        'chat.layout.diffPanelEmpty',
        'Select a file on the right to inspect the full diff',
    );
    const diffViewModeLabel = translateWithFallback('chat.layout.diffViewMode', 'Diff view mode');
    const diffUnifiedViewLabel = translateWithFallback('chat.layout.diffUnifiedView', 'Unified diff view');
    const diffSplitViewLabel = translateWithFallback('chat.layout.diffSplitView', 'Split diff view');
    const diffLineWrapLabel = translateWithFallback('chat.layout.diffLineWrap', 'Wrap diff lines');
    const diffLineNoWrapLabel = translateWithFallback(
        'chat.layout.diffLineNoWrap',
        'Do not wrap diff lines; use horizontal scrolling',
    );
    const diffLineSummaryLabel = edit
        ? translateWithFallback('chat.layout.diffLineSummary', `${edit.diffPreviewLines.length} diff lines`, {
            count: edit.diffPreviewLines.length,
        })
        : '';
    const openFileLabel = edit
        ? translateWithFallback(
            'tools.openFileForPath',
            `Open file: ${edit.displayPath}`,
            {file: edit.displayPath},
        )
        : translateWithFallback('tools.openFile', 'Open file');
    const copyPathLabel = edit
        ? translateWithFallback(
            'tools.copyPathForPath',
            `Copy path: ${edit.displayPath}`,
            {file: edit.displayPath},
        )
        : translateWithFallback('tools.copyPath', 'Copy path');
    const copiedPathLabel = edit
        ? translateWithFallback(
            'tools.copiedPathForPath',
            `Copied path: ${edit.displayPath}`,
            {file: edit.displayPath},
        )
        : translateWithFallback('tools.copied', 'Copied');
    const activeCopyLabel = copied ? copiedPathLabel : copyPathLabel;
    const collapseDiffPanelLabel = translateWithFallback('chat.layout.collapseDiffPanel', 'Collapse file diff panel');

    return (
        <section
            ref={ref}
            className="chat-diff-review-pane chat-diff-review-pane-focus-target"
            aria-label={diffPanelLabel}
            data-chat-diff-review-pane="true"
            tabIndex={-1}
        >
            <div className="chat-diff-review-header">
                <div className="chat-diff-review-title">
                    <FileDiff size={14} />
                    <div className="min-w-0">
                        <div className="chat-diff-review-heading">{diffPanelBaseLabel}</div>
                        <div className="chat-diff-review-path" title={edit?.displayPath}>
                            {edit?.displayPath ?? diffPanelEmptyLabel}
                        </div>
                    </div>
                </div>
                <div className="chat-diff-review-actions">
                    {edit && (
                        <div className="chat-diff-review-stats" title={diffLineSummaryLabel}>
                            <span className="edit-stat-added">+{edit.additions}</span>
                            <span className="edit-stat-deleted">-{edit.deletions}</span>
                        </div>
                    )}
                    <div className="status-diff-mode-toggle" role="group" aria-label={diffViewModeLabel}>
                        <button
                            type="button"
                            className={diffModeButtonClass('unified')}
                            title={diffUnifiedViewLabel}
                            aria-label={diffUnifiedViewLabel}
                            aria-pressed={mode === 'unified'}
                            onClick={() => onModeChange('unified')}
                        >
                            <Rows3 size={12} />
                        </button>
                        <button
                            type="button"
                            className={diffModeButtonClass('split')}
                            title={diffSplitViewLabel}
                            aria-label={diffSplitViewLabel}
                            aria-pressed={mode === 'split'}
                            onClick={() => onModeChange('split')}
                        >
                            <Columns2 size={12} />
                            <span className="diff-mode-color-bars" aria-hidden="true">
                                <span className="diff-mode-color-bar deleted" />
                                <span className="diff-mode-color-bar added" />
                            </span>
                        </button>
                    </div>
                    <button
                        type="button"
                        className={cn('status-diff-mode-button chat-diff-review-wrap-toggle', wrapLines && 'active')}
                        title={wrapLines ? diffLineNoWrapLabel : diffLineWrapLabel}
                        aria-label={wrapLines ? diffLineNoWrapLabel : diffLineWrapLabel}
                        aria-pressed={wrapLines}
                        onClick={() => onWrapLinesChange(!wrapLines)}
                    >
                        {wrapLines ? <TextWrap size={12} /> : <ScrollText size={12} />}
                    </button>
                    <button
                        type="button"
                        className="chat-diff-review-open"
                        title={openFileLabel}
                        aria-label={openFileLabel}
                        disabled={!edit}
                        onClick={handleOpenFile}
                    >
                        <ExternalLink size={13} />
                    </button>
                    <button
                        type="button"
                        className={cn('chat-diff-review-copy', copied && 'copied')}
                        title={activeCopyLabel}
                        aria-label={activeCopyLabel}
                        disabled={!edit}
                        onClick={() => void handleCopyPath()}
                    >
                        {copied ? <Check size={13} /> : <Copy size={13} />}
                    </button>
                    {onCollapse && (
                        <button
                            type="button"
                            className="chat-diff-review-collapse"
                            title={collapseDiffPanelLabel}
                            aria-label={collapseDiffPanelLabel}
                            onClick={onCollapse}
                        >
                            <PanelRightClose size={13} />
                        </button>
                    )}
                </div>
            </div>

            <div className="chat-diff-review-body">
                {edit && edit.diffPreviewLines.length > 0 ? (
                    <EditDiffPreview
                        filePath={edit.displayPath}
                        additions={edit.additions}
                        deletions={edit.deletions}
                        lines={edit.diffPreviewLines}
                        mode={mode}
                        wrapLines={wrapLines}
                        variant="panel"
                        lineLimit={undefined}
                    />
                ) : (
                    <div className="chat-diff-review-empty">
                        <FileDiff size={18} />
                        <span>{diffPanelEmptyLabel}</span>
                    </div>
                )}
            </div>
        </section>
    );
});

export default ChatDiffReviewPane;
