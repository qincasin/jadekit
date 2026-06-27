// ReadToolGroupBlock - Read 工具分组块

import {type KeyboardEvent, memo, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {ChevronDown, ChevronRight, FileSearch} from 'lucide-react';
import type {ToolResultBlock, ToolUseBlock} from '../../types/chat';
import {
    formatToolExecutionStatusSummary,
    getGroupStatus,
    getToolGroupBulkActionState,
    getToolGroupExpandedIndices,
    isToolBlockToggleActivationKey,
    summarizeToolResultStatuses,
    toggleToolGroupExpandedIndex,
} from '../../utils/toolGrouping';
import {getToolLineInfo, resolveToolTarget, summarizeReadGroupHeader} from '../../utils/toolPresentation';
import {getFileIcon, getFolderIcon} from '../../utils/fileIcons';
import {openFile} from '../../utils/bridge';
import {useChatStore} from '../../stores/useChatStore';
import ReadToolBlock from './ReadToolBlock';

export interface ReadToolGroupBlockProps {
  blocks: ToolUseBlock[];
  findToolResult: (toolId: string) => ToolResultBlock | null | undefined;
  compact?: boolean;
}

const ReadToolGroupBlock = memo(function ReadToolGroupBlock({
  blocks,
  findToolResult,
  compact = false,
}: ReadToolGroupBlockProps) {
  const { t } = useTranslation();
  const [groupExpanded, setGroupExpanded] = useState(!compact);
  const [expandedIndices, setExpandedIndices] = useState<Set<number>>(new Set());
  const currentCwd = useChatStore((state) => state.currentCwd);

  // 计算整体状态
  const status = getGroupStatus(blocks, findToolResult);
  const header = summarizeReadGroupHeader(blocks);
  const statusSummary = summarizeToolResultStatuses(blocks, findToolResult);
  const statusSummaryText = formatToolExecutionStatusSummary(statusSummary, {
    success: t('tools.success'),
    failed: t('tools.failed'),
    pending: t('tools.pending'),
  });
  const headerSecondarySummary = [header.secondarySummary, statusSummaryText].filter(Boolean).join(' · ');
  const groupToggleTarget = [
    header.primarySummary || t('tools.read'),
    headerSecondarySummary,
  ].filter(Boolean).join(' · ');
  const groupToggleLabel = t('tools.readGroupDetailsToggle', { target: groupToggleTarget });
  const expandAllLabel = t('tools.expandAllInGroup', { target: groupToggleTarget });
  const collapseAllLabel = t('tools.collapseAllInGroup', { target: groupToggleTarget });
  const {allItemsExpanded, noItemsExpanded} = getToolGroupBulkActionState(blocks.length, expandedIndices);

  // 全部展开/折叠
  const toggleAll = (expand: boolean) => {
    setExpandedIndices(expand ? getToolGroupExpandedIndices(blocks.length) : new Set());
  };

  // 切换单个
  const toggleItem = (index: number) => {
    setExpandedIndices((current) => toggleToolGroupExpandedIndex(blocks.length, current, index));
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
          <FileSearch className="tool-title-lucide" aria-hidden="true" />
          <span className="tool-title-text">{t('tools.read')}</span>
          {header.primarySummary && (
            <span
              className="tool-title-summary"
              title={header.primarySummary}
              aria-label={header.primarySummary}
            >
              {header.primarySummary}
            </span>
          )}
          <span
            className="tool-title-secondary-summary"
            title={headerSecondarySummary}
            aria-label={headerSecondarySummary}
          >
            {headerSecondarySummary}
          </span>
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
            {blocks.map((block, index) => {
              const result = findToolResult(block.id);
              const target = resolveToolTarget(block.input);
              const lineInfo = getToolLineInfo(block.input, target);
              const filePath = target?.displayPath || '';
              const openFileLabel = target?.isFile ? `${t('tools.openFile')}: ${filePath}` : '';
              const itemToggleTarget = filePath || target?.rawPath || t('tools.read');
              const itemToggleLabel = t('tools.readGroupItemDetailsToggle', { target: itemToggleTarget });
              const lineSummary = lineInfo.start
                ? lineInfo.end && lineInfo.end !== lineInfo.start
                  ? `L${lineInfo.start}-${lineInfo.end}`
                  : `L${lineInfo.start}`
                : '';
              const lineSummaryLabel = lineSummary ? `Read lines: ${itemToggleTarget} · ${lineSummary}` : '';
              const isExpanded = expandedIndices.has(index);

              // 文件图标
              const isDirectory = target?.isDirectory ?? false;
              const fileIconSvg = target
                ? isDirectory
                  ? getFolderIcon(target.cleanFileName)
                  : getFileIcon(target.cleanFileName.split('.').pop() || '', target.cleanFileName)
                : '';

              // 单个工具状态
              const isCompleted = result !== undefined && result !== null;
              const isError = isCompleted && result?.is_error === true;
              const itemStatus = isError ? 'error' : isCompleted ? 'completed' : 'pending';
              const itemStatusText = itemStatus === 'error'
                ? t('tools.failed')
                : itemStatus === 'completed'
                  ? t('tools.success')
                  : t('tools.pending');
              const itemStatusLabel = `Read: ${itemToggleTarget} · ${itemStatusText}`;

              return (
                <div key={block.id} className="task-group-item">
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
                      {target?.isFile ? (
                        <button
                          type="button"
                          className="task-group-item-file file-path-button clickable-file"
                          title={openFileLabel}
                          aria-label={openFileLabel}
                          onClick={(event) => {
                            event.stopPropagation();
                            void openFile(target.openPath, lineInfo.start, lineInfo.end, currentCwd);
                          }}
                          onKeyDown={(event) => {
                            event.stopPropagation();
                          }}
                        >
                          {filePath}
                        </button>
                      ) : (
                        <span
                          className="task-group-item-file"
                          title={target?.rawPath ?? filePath}
                          aria-label={target?.rawPath ?? filePath}
                        >
                          {filePath}
                        </span>
                      )}
                      {lineSummary && (
                        <span
                          className="task-group-item-secondary"
                          title={lineSummaryLabel}
                          aria-label={lineSummaryLabel}
                        >
                          {lineSummary}
                        </span>
                      )}
                    </div>
                    <div className="task-group-item-status">
                      <span
                        className={`tool-state-pill ${itemStatus}`}
                        title={itemStatusLabel}
                        aria-label={itemStatusLabel}
                      >
                        {itemStatusText}
                      </span>
                      {isExpanded
                        ? <ChevronDown className="task-group-item-chevron-icon" aria-hidden="true" />
                        : <ChevronRight className="task-group-item-chevron-icon" aria-hidden="true" />}
                    </div>
                  </div>

                  {/* 展开的内容 */}
                  {isExpanded && (
                    <div className="task-group-item-content">
                      <ReadToolBlock
                        name={block.name}
                        input={block.input}
                        result={result}
                        toolId={block.id}
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

export default ReadToolGroupBlock;
