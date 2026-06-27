// EditToolBlock - 文件编辑工具块

import {memo, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {PencilLine} from 'lucide-react';
import type {ToolResultBlock} from '../../types/chat';
import type {ToolInput} from '../../types/tools';
import {useIsToolDenied} from '../../hooks/useIsToolDenied';
import {useChatStore} from '../../stores/useChatStore';
import {collectEditToolItems, resolveToolTarget} from '../../utils/toolPresentation';
import {getFileIcon} from '../../utils/fileIcons';
import {copyToClipboard, openFile} from '../../utils/bridge';
import {isToolBlockToggleActivationKey} from '../../utils/toolGrouping';
import EditDiffPreview from './EditDiffPreview';

export interface EditToolBlockProps {
  name?: string;
  input?: ToolInput;
  result?: ToolResultBlock | null;
  toolId?: string;
  compact?: boolean;
}

const EditToolBlock = memo(function EditToolBlock({
  name,
  input,
  result,
  toolId,
  compact = false,
}: EditToolBlockProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const isDenied = useIsToolDenied(toolId);
  const currentCwd = useChatStore((state) => state.currentCwd);

  if (!input) {
    return null;
  }

  // 解析文件路径
  const primaryItem = collectEditToolItems(
    [{ type: 'tool_use', id: toolId ?? 'edit', name: name ?? 'Edit', input }],
    () => result,
  )[0];
  const target = resolveToolTarget(input);
  const filePath = primaryItem?.filePath ?? target?.rawPath ?? '';
  const displayPath = primaryItem?.displayPath ?? target?.displayPath ?? filePath;
  const openPath = primaryItem?.openPath ?? target?.openPath;
  const cleanFileName = primaryItem?.cleanFileName ?? target?.cleanFileName ?? filePath;

  // 提取编辑内容
  const oldString = primaryItem?.oldString ?? (input.old_string as string) ?? '';
  const newString = primaryItem?.newString ?? (input.new_string as string) ?? '';
  const hasChanges = oldString || newString;
  const additions = primaryItem?.additions ?? 0;
  const deletions = primaryItem?.deletions ?? 0;
  const hasEditStats = additions > 0 || deletions > 0;
  const diffPreviewLines = primaryItem?.diffPreviewLines ?? [];

  // 状态计算
  const isCompleted = (result !== undefined && result !== null) || isDenied;
  const isError = isDenied || (isCompleted && result?.is_error === true);
  const status = isError ? 'error' : isCompleted ? 'completed' : 'pending';
  const copyPathButtonLabel = t('tools.copyPath');
  const copyPathActionLabel = t('tools.copyPathForPath', { file: displayPath || filePath });
  const openFileLabel = openPath ? `${t('tools.openFile')}: ${displayPath || filePath}` : '';
  const headerToggleTarget = displayPath || filePath || t('tools.editFile');
  const headerToggleLabel = t('tools.editDetailsToggle', { target: headerToggleTarget });
  const editStatsFallbackLabel = `Edit stats: ${displayPath || filePath} · +${additions} / -${deletions}`;
  const translatedEditStatsLabel = hasEditStats
    ? t('chat.layout.inputStatusEditFileStats', {
        defaultValue: editStatsFallbackLabel,
        file: displayPath || filePath,
        additions,
        deletions,
      })
    : '';
  const editStatsLabel = translatedEditStatsLabel === 'chat.layout.inputStatusEditFileStats'
    || translatedEditStatsLabel.includes('{{')
    ? editStatsFallbackLabel
    : translatedEditStatsLabel;

  // 文件图标
  const fileIconSvg = cleanFileName
    ? getFileIcon(cleanFileName.split('.').pop() || '', cleanFileName)
    : '';

  // 文件路径点击
  const handleFileClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (openPath) {
      void openFile(openPath, primaryItem?.lineStart, primaryItem?.lineEnd, currentCwd);
    }
  };

  // 复制路径
  const handleCopyPath = async (event?: React.MouseEvent) => {
    event?.stopPropagation();
    await copyToClipboard(filePath);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const toggleExpanded = () => setExpanded((prev) => !prev);

  const handleHeaderKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!isToolBlockToggleActivationKey(event.key)) return;
    event.preventDefault();
    toggleExpanded();
  };

  const detailContent = (
    <div className="task-content-wrapper">
      {/* 文件路径 */}
      <div className="tool-section">
        <div className="tool-section-label">{t('tools.filePath')}:</div>
        <div className="file-path-display">
          <code>{filePath}</code>
        </div>
      </div>

      {/* Diff 预览 */}
      {hasChanges && (
        <div className="tool-section">
          <div className="tool-section-label">{t('tools.changes')}:</div>
          <div className="diff-preview">
            {oldString && (
              <div className="diff-line removed">
                <span className="diff-marker">-</span>
                <span className="diff-content">{oldString}</span>
              </div>
            )}
            {newString && (
              <div className="diff-line added">
                <span className="diff-marker">+</span>
                <span className="diff-content">{newString}</span>
              </div>
            )}
          </div>
        </div>
      )}

      {/* 其他参数 */}
      {Object.entries(input)
        .filter(([key]) => !['file_path', 'path', 'target_file', 'old_string', 'new_string', 'command', 'workdir', 'description'].includes(key))
        .map(([key, value]) => (
          <div key={key} className="tool-section">
            <div className="tool-section-label">{key}:</div>
            <div className="tool-param-value">
              {typeof value === 'object' && value !== null
                ? JSON.stringify(value, null, 2)
                : String(value)}
            </div>
          </div>
        ))}

      {/* 操作按钮 */}
      <div className="tool-actions">
        {openPath && (
          <button
            type="button"
            className="btn btn-sm btn-ghost"
            title={openFileLabel}
            aria-label={openFileLabel}
            onClick={(event) => {
              event.stopPropagation();
              void openFile(openPath, primaryItem?.lineStart, primaryItem?.lineEnd, currentCwd);
            }}
          >
            {t('tools.openFile')}
          </button>
        )}
        <button
          type="button"
          className={`btn btn-sm ${copied ? 'btn-success' : 'btn-ghost'}`}
          title={copyPathActionLabel}
          aria-label={copyPathActionLabel}
          onClick={handleCopyPath}
        >
          {copied ? t('tools.copied') : copyPathButtonLabel}
        </button>
      </div>
    </div>
  );

  return (
    <div className={`task-container ${compact ? 'task-container-compact' : ''}`}>
      <div
        className={compact ? 'task-header task-header-compact' : 'task-header'}
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        aria-label={headerToggleLabel}
        title={headerToggleLabel}
        onClick={toggleExpanded}
        onKeyDown={handleHeaderKeyDown}
        style={{ cursor: 'pointer' }}
      >
        <div className="task-title-section">
          <PencilLine className="tool-title-lucide" aria-hidden="true" />
          <span className="tool-title-text">{t('tools.editFile')}</span>
          {openPath ? (
            <button
              type="button"
              className="tool-title-summary file-path-link file-path-button clickable-file edit-diff-hover-trigger"
              onClick={handleFileClick}
              onKeyDown={(event) => {
                event.stopPropagation();
              }}
              title={openFileLabel}
              aria-label={openFileLabel}
            >
              {fileIconSvg && (
                <span
                  className="file-icon"
                  dangerouslySetInnerHTML={{ __html: fileIconSvg }}
                />
              )}
              <span className="edit-diff-hover-label">{displayPath}</span>
              {hasEditStats && (
                <span className="edit-item-stats" title={editStatsLabel} aria-label={editStatsLabel}>
                  <span className="edit-stat-added" aria-hidden="true">+{additions}</span>
                  <span className="edit-stat-deleted" aria-hidden="true">-{deletions}</span>
                </span>
              )}
              <EditDiffPreview
                filePath={displayPath}
                additions={additions}
                deletions={deletions}
                lines={diffPreviewLines}
              />
            </button>
          ) : (
            <span
              className="tool-title-summary file-path-link edit-diff-hover-trigger"
              title={filePath}
              aria-label={filePath}
            >
              {fileIconSvg && (
                <span
                  className="file-icon"
                  dangerouslySetInnerHTML={{ __html: fileIconSvg }}
                />
              )}
              <span className="edit-diff-hover-label">{displayPath}</span>
              {hasEditStats && (
                <span className="edit-item-stats" title={editStatsLabel} aria-label={editStatsLabel}>
                  <span className="edit-stat-added" aria-hidden="true">+{additions}</span>
                  <span className="edit-stat-deleted" aria-hidden="true">-{deletions}</span>
                </span>
              )}
              <EditDiffPreview
                filePath={displayPath}
                additions={additions}
                deletions={deletions}
                lines={diffPreviewLines}
              />
            </span>
          )}
          {isDenied && <span className="tool-title-summary text-error">• {t('tools.denied')}</span>}
        </div>
        <div className={`tool-status-indicator ${status}`} />
      </div>

      {expanded && (
        <div className={`task-details ${compact ? 'task-details-compact' : ''}`}>
          {detailContent}
        </div>
      )}
    </div>
  );
});

export default EditToolBlock;
