// EditToolGroupBlock - Edit 工具分组块

import {type KeyboardEvent, memo, useMemo, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {ChevronDown, ChevronRight, PencilLine} from 'lucide-react';
import type {ToolResultBlock, ToolUseBlock} from '../../types/chat';
import {
    formatToolExecutionStatusSummary,
    getGroupStatus,
    getToolGroupBulkActionState,
    getToolGroupExpandedIndices,
    isToolBlockToggleActivationKey,
    summarizeToolExecutionStatuses,
    toggleToolGroupExpandedIndex,
} from '../../utils/toolGrouping';
import {collectEditToolItems, mergeEditToolItemsByFile} from '../../utils/toolPresentation';
import {getFileIcon} from '../../utils/fileIcons';
import {openFile} from '../../utils/bridge';
import {useChatStore} from '../../stores/useChatStore';
import EditToolBlock from './EditToolBlock';
import EditDiffPreview from './EditDiffPreview';

export interface EditToolGroupBlockProps {
  blocks: ToolUseBlock[];
  findToolResult: (toolId: string) => ToolResultBlock | null | undefined;
  compact?: boolean;
}

const EditToolGroupBlock = memo(function EditToolGroupBlock({
  blocks,
  findToolResult,
  compact = false,
}: EditToolGroupBlockProps) {
  const { t } = useTranslation();
  const [groupExpanded, setGroupExpanded] = useState(!compact);
  const [expandedIndices, setExpandedIndices] = useState<Set<number>>(new Set());
  const currentCwd = useChatStore((state) => state.currentCwd);

  // 计算整体状态
  const status = getGroupStatus(blocks, findToolResult);
  const editItems = useMemo(
    () => mergeEditToolItemsByFile(collectEditToolItems(blocks, findToolResult)),
    [blocks, findToolResult],
  );
  const totalAdditions = editItems.reduce((sum, item) => sum + item.additions, 0);
  const totalDeletions = editItems.reduce((sum, item) => sum + item.deletions, 0);
  const hasTotalEditStats = totalAdditions > 0 || totalDeletions > 0;
  const statusSummary = summarizeToolExecutionStatuses(editItems);
  const statusSummaryText = formatToolExecutionStatusSummary(statusSummary, {
    success: t('tools.success'),
    failed: t('tools.failed'),
    pending: t('tools.pending'),
  });
  const groupToggleTarget = [
    editItems[0]?.displayPath || t('tools.editBatchFiles'),
    statusSummaryText,
  ].filter(Boolean).join(' · ');
  const groupToggleLabel = t('tools.editGroupDetailsToggle', { target: groupToggleTarget });
  const expandAllLabel = t('tools.expandAllInGroup', { target: groupToggleTarget });
  const collapseAllLabel = t('tools.collapseAllInGroup', { target: groupToggleTarget });
  const {allItemsExpanded, noItemsExpanded} = getToolGroupBulkActionState(editItems.length, expandedIndices);
  const getEditStatsLabel = (target: string, additions: number, deletions: number) => {
    const fallbackLabel = `Edit stats: ${target} · +${additions} / -${deletions}`;
    const translatedLabel = t('chat.layout.inputStatusEditFileStats', {
      defaultValue: fallbackLabel,
      file: target,
      additions,
      deletions,
    });
    return translatedLabel === 'chat.layout.inputStatusEditFileStats' || translatedLabel.includes('{{')
      ? fallbackLabel
      : translatedLabel;
  };
  const groupStatsTarget = `${editItems[0]?.displayPath || t('tools.editBatchFiles')} · ${editItems.length} files`;
  const totalStatsLabel = hasTotalEditStats
    ? getEditStatsLabel(groupStatsTarget, totalAdditions, totalDeletions)
    : '';

  if (editItems.length === 0) {
    return null;
  }

  // 全部展开/折叠
  const toggleAll = (expand: boolean) => {
    setExpandedIndices(expand ? getToolGroupExpandedIndices(editItems.length) : new Set());
  };

  // 切换单个
  const toggleItem = (index: number) => {
    setExpandedIndices((current) => toggleToolGroupExpandedIndex(editItems.length, current, index));
  };

  const handleItemKeyDown = (event: KeyboardEvent<HTMLDivElement>, index: number) => {
    if (!isToolBlockToggleActivationKey(event.key)) return;

    event.preventDefault();
    toggleItem(index);
  };

  return (
    <div className={`task-container task-group-container ${compact ? 'task-container-compact task-group-container-compact' : ''}`}>
      {/* 分组标题 */}
      <div
        className={`task-header task-group-header ${compact ? 'task-group-header-compact' : ''}`}
        role="button"
        tabIndex={0}
        aria-expanded={groupExpanded}
        aria-label={groupToggleLabel}
        title={groupToggleLabel}
        onClick={() => setGroupExpanded((prev) => !prev)}
        onKeyDown={(event) => {
          if (isToolBlockToggleActivationKey(event.key)) {
            event.preventDefault();
            setGroupExpanded((prev) => !prev);
          }
        }}
      >
        <div className="task-title-section">
          <PencilLine className="tool-title-lucide" aria-hidden="true" />
          <span className="tool-title-text">{t('tools.editBatchFiles')}</span>
          <span className="tool-title-summary">
            ({editItems.length})
          </span>
          {hasTotalEditStats && (
            <span className="edit-total-stats" title={totalStatsLabel} aria-label={totalStatsLabel}>
              <span className="edit-stat-added" aria-hidden="true">+{totalAdditions}</span>
              <span className="edit-stat-deleted" aria-hidden="true">-{totalDeletions}</span>
            </span>
          )}
          {statusSummaryText && (
            <span className="tool-title-secondary-summary" title={statusSummaryText}>
              {statusSummaryText}
            </span>
          )}
        </div>
        <div className="task-group-header-status">
          <div className={`tool-status-indicator ${status}`} />
          {groupExpanded
            ? <ChevronDown className="task-group-header-chevron" aria-hidden="true" />
            : <ChevronRight className="task-group-header-chevron" aria-hidden="true" />}
        </div>
      </div>

      {/* 分组列表 */}
      {groupExpanded && (
        <>
          <div className="task-group-list">
            {editItems.map((item, index) => {
              const isExpanded = expandedIndices.has(index);

              // 文件图标
              const fileIconSvg = getFileIcon(item.cleanFileName.split('.').pop() || '', item.cleanFileName);

              // 单个工具状态
              const itemStatus = item.isError ? 'error' : item.isCompleted ? 'completed' : 'pending';
              const openFileLabel = `${t('tools.openFile')}: ${item.displayPath}`;
              const itemToggleLabel = t('tools.editGroupItemDetailsToggle', { target: item.displayPath });
              const hasItemStats = item.additions > 0 || item.deletions > 0;
              const itemStatsLabel = hasItemStats
                ? getEditStatsLabel(item.displayPath, item.additions, item.deletions)
                : '';

              return (
                <div key={item.id} className="task-group-item">
                  {/* 单项标题 */}
                  <div
                    className="task-group-item-header"
                    role="button"
                    tabIndex={0}
                    aria-expanded={isExpanded}
                    aria-label={itemToggleLabel}
                    title={itemToggleLabel}
                    onClick={() => toggleItem(index)}
                    onKeyDown={(event) => handleItemKeyDown(event, index)}
                  >
                    <div className="task-group-item-title">
                      <span className="task-group-item-number">{index + 1}.</span>
                      {fileIconSvg && (
                        <span
                          className="file-icon"
                          dangerouslySetInnerHTML={{ __html: fileIconSvg }}
                        />
                      )}
                      <button
                        type="button"
                        className="task-group-item-file file-path-button clickable-file edit-diff-hover-trigger"
                        title={openFileLabel}
                        aria-label={openFileLabel}
                        onClick={(event) => {
                          event.stopPropagation();
                          void openFile(item.openPath, item.lineStart, item.lineEnd, currentCwd);
                        }}
                        onKeyDown={(event) => {
                          event.stopPropagation();
                        }}
                      >
                        <span className="edit-diff-hover-label">{item.displayPath}</span>
                        <EditDiffPreview
                          filePath={item.displayPath}
                          additions={item.additions}
                          deletions={item.deletions}
                          lines={item.diffPreviewLines}
                        />
                      </button>
                    </div>
                    <div className="task-group-item-status">
                      {hasItemStats && (
                        <span className="task-group-item-badge edit-item-stats" title={itemStatsLabel} aria-label={itemStatsLabel}>
                          <span className="edit-stat-added" aria-hidden="true">+{item.additions}</span>
                          <span className="edit-stat-deleted" aria-hidden="true">-{item.deletions}</span>
                        </span>
                      )}
                      <span className={`tool-state-pill ${itemStatus}`}>
                        {itemStatus === 'error' ? t('tools.failed') : itemStatus === 'completed' ? t('tools.success') : t('tools.pending')}
                      </span>
                      {isExpanded
                        ? <ChevronDown className="task-group-item-chevron-icon" aria-hidden="true" />
                        : <ChevronRight className="task-group-item-chevron-icon" aria-hidden="true" />}
                    </div>
                  </div>

                  {/* 展开的内容 */}
                  {isExpanded && (
                    <div className="task-group-item-content">
                      <EditToolBlock
                        name={item.name}
                        input={item.input}
                        result={item.result}
                        toolId={item.toolId}
                        compact
                      />
                    </div>
                  )}
                </div>
              );
            })}
          </div>

          {/* 分组操作 */}
          <div className="task-group-actions">
            <button
              type="button"
              className="btn btn-sm btn-ghost"
              title={expandAllLabel}
              aria-label={expandAllLabel}
              disabled={allItemsExpanded}
              onClick={() => toggleAll(true)}
            >
              {t('tools.expandAll')}
            </button>
            <button
              type="button"
              className="btn btn-sm btn-ghost"
              title={collapseAllLabel}
              aria-label={collapseAllLabel}
              disabled={noItemsExpanded}
              onClick={() => toggleAll(false)}
            >
              {t('tools.collapseAll')}
            </button>
          </div>
        </>
      )}
    </div>
  );
});

export default EditToolGroupBlock;
