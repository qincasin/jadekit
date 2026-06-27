// BashToolGroupBlock - Bash 工具分组块

import {type KeyboardEvent, memo, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {ChevronDown, ChevronRight, Terminal} from 'lucide-react';
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
import {summarizeBashGroupHeader, summarizeCommand, summarizeGroupBashItemResult} from '../../utils/toolPresentation';
import BashToolBlock from './BashToolBlock';

export interface BashToolGroupBlockProps {
  blocks: ToolUseBlock[];
  findToolResult: (toolId: string) => ToolResultBlock | null | undefined;
  compact?: boolean;
}

const BashToolGroupBlock = memo(function BashToolGroupBlock({
  blocks,
  findToolResult,
  compact = false,
}: BashToolGroupBlockProps) {
  const { t } = useTranslation();
  const [groupExpanded, setGroupExpanded] = useState(!compact);
  const [expandedIndices, setExpandedIndices] = useState<Set<number>>(new Set());

  // 计算整体状态
  const status = getGroupStatus(blocks, findToolResult);
  const headerSummary = summarizeBashGroupHeader(blocks, findToolResult);
  const statusSummary = formatToolExecutionStatusSummary(
    summarizeToolResultStatuses(blocks, findToolResult),
    {
      success: t('tools.success'),
      failed: t('tools.failed'),
      pending: t('tools.pending'),
    },
  );
  const baseGroupToggleTarget = headerSummary.primarySummary || t('tools.commandCount', { count: headerSummary.totalCount });
  const groupToggleTarget = statusSummary
    ? `${baseGroupToggleTarget} · ${statusSummary}`
    : baseGroupToggleTarget;
  const groupToggleLabel = t('tools.bashGroupDetailsToggle', { target: groupToggleTarget });
  const expandAllLabel = t('tools.expandAllInGroup', { target: groupToggleTarget });
  const collapseAllLabel = t('tools.collapseAllInGroup', { target: groupToggleTarget });
  const commandCountLabel = t('tools.commandCount', { count: blocks.length });
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
          <Terminal className="tool-title-lucide" aria-hidden="true" />
          <span className="tool-title-text">{t('tools.runCommand')}</span>
          <span className="tool-command-chip tool-command-run" title={commandCountLabel} aria-label={commandCountLabel}>{blocks.length}</span>
          {headerSummary.primarySummary && (
            <span className="tool-title-summary" title={headerSummary.primarySummary} aria-label={headerSummary.primarySummary}>
              {headerSummary.primarySummary}
            </span>
          )}
          <span className="tool-title-secondary-summary" title={statusSummary} aria-label={statusSummary}>
            {statusSummary}
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
              const command = (block.input.command as string) || '';
              const commandSummary = summarizeCommand(command);
              const resultSummary = summarizeGroupBashItemResult(result);
              const isExpanded = expandedIndices.has(index);
              const itemToggleTarget = commandSummary.summary || commandSummary.label || t('tools.runCommand');
              const itemToggleLabel = t('tools.bashGroupItemDetailsToggle', { target: itemToggleTarget });
              const commandLabel = command || itemToggleTarget;

              // 单个工具状态
              const isCompleted = result !== undefined && result !== null;
              const isError = isCompleted && result?.is_error === true;
              const itemStatus = isError ? 'error' : isCompleted ? 'completed' : 'pending';
              const itemStatusText = itemStatus === 'error' ? t('tools.failed') : itemStatus === 'completed' ? t('tools.success') : t('tools.pending');
              const itemStatusLabel = `Bash: ${itemToggleTarget} · ${itemStatusText}`;

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
                      <span className={`tool-command-chip ${commandSummary.accentClass}`}>
                        {commandSummary.label}
                      </span>
                      <span className="task-group-item-command" title={commandLabel} aria-label={commandLabel}>
                        {commandSummary.summary}
                      </span>
                      {resultSummary && (
                        <span className={`task-group-item-secondary ${isError ? 'error' : ''}`} title={resultSummary} aria-label={resultSummary}>
                          {resultSummary}
                        </span>
                      )}
                    </div>
                    <div className="task-group-item-status">
                      <span className={`tool-state-pill ${itemStatus}`} title={itemStatusLabel} aria-label={itemStatusLabel}>
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
                      <BashToolBlock
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

export default BashToolGroupBlock;
