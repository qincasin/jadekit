// SearchToolGroupBlock - 搜索工具分组块（Grep/Glob）

import {type KeyboardEvent, memo, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {ChevronDown, ChevronRight, Search} from 'lucide-react';
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
import {
    extractResultText,
    summarizeSearchGroupHeader,
    summarizeSearchInput,
    summarizeSearchResultText,
} from '../../utils/toolPresentation';
import {openFile} from '../../utils/bridge';
import {useChatStore} from '../../stores/useChatStore';
import GenericToolBlock from './GenericToolBlock';

export interface SearchToolGroupBlockProps {
  blocks: ToolUseBlock[];
  findToolResult: (toolId: string) => ToolResultBlock | null | undefined;
  compact?: boolean;
}

const formatOpenFileLabel = (openFileLabel: string, path: string) => `${openFileLabel}: ${path}`;

const formatSearchResultLabel = (file: {path: string; lineStart?: number; snippet?: string}) => {
  const parts = [file.path];
  if (file.lineStart) {
    parts.push(`L${file.lineStart}`);
  }
  if (file.snippet) {
    parts.push(file.snippet);
  }

  return `Open search result: ${parts.join(' · ')}`;
};

const SearchToolGroupBlock = memo(function SearchToolGroupBlock({
  blocks,
  findToolResult,
  compact = false,
}: SearchToolGroupBlockProps) {
  const { t } = useTranslation();
  const [groupExpanded, setGroupExpanded] = useState(!compact);
  const [expandedIndices, setExpandedIndices] = useState<Set<number>>(new Set());
  const currentCwd = useChatStore((state) => state.currentCwd);

  // 计算整体状态
  const status = getGroupStatus(blocks, findToolResult);
  const firstPattern = summarizeSearchInput(blocks[0]?.input ?? {});
  const summaryHint = firstPattern || t('tools.searchQuery', {count: blocks.length});
  const statusSummary = summarizeToolResultStatuses(blocks, findToolResult);
  const statusSummaryText = formatToolExecutionStatusSummary(statusSummary, {
    success: t('tools.success'),
    failed: t('tools.failed'),
    pending: t('tools.pending'),
  });
  const groupToggleTarget = [summaryHint, statusSummaryText].filter(Boolean).join(' · ');
  const groupToggleLabel = t('tools.searchGroupDetailsToggle', { target: groupToggleTarget });
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
          <Search className="tool-title-lucide" aria-hidden="true" />
          <span className="tool-title-text">{t('tools.search')}</span>
          <span className="tool-command-chip tool-command-search">
            {t('tools.searchFind')}
          </span>
          {summaryHint && (
            <span
              className="tool-title-summary"
              title={summaryHint}
              aria-label={summaryHint}
            >
              {summaryHint}
            </span>
          )}
          {statusSummaryText && (
            <span
              className="tool-title-secondary-summary"
              title={statusSummaryText}
              aria-label={statusSummaryText}
            >
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
            {blocks.map((block, index) => {
              const result = findToolResult(block.id);
              const pattern = summarizeSearchInput(block.input);
              const summary = result ? summarizeSearchResultText(extractResultText(result)) : {
                matchCount: 0,
                fileCount: 0,
                files: [],
              };
              const omittedResultCount = summary.omittedResultCount ?? 0;
              const omittedResultLabel = omittedResultCount > 0
                ? t('tools.searchMoreResults', { count: omittedResultCount })
                : '';
              const header = summarizeSearchGroupHeader(block.name, pattern, summary);
              const isExpanded = expandedIndices.has(index);
              const firstFile = summary.files[0];
              const firstFileOpenLabel = firstFile
                ? formatOpenFileLabel(t('tools.openFile'), firstFile.path)
                : '';
              const itemToggleTarget = header.primarySummary || firstFile?.path || block.name;
              const itemToggleLabel = t('tools.searchGroupItemDetailsToggle', { target: itemToggleTarget });
              const patternLabel = header.primarySummary
                ? `Search query: ${header.primarySummary}`
                : `Search tool: ${block.name}`;
              const resultSummaryLabel = header.secondarySummary
                ? `Search results for ${itemToggleTarget}: ${header.secondarySummary}`
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
              const itemStatusLabel = `Search: ${itemToggleTarget} · ${itemStatusText}`;

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
                      <span className="tool-command-chip tool-command-search">
                        {block.name.toLowerCase().includes('glob') ? t('tools.searchGlob') : t('tools.searchFind')}
                      </span>
                      <span
                        className="task-group-item-pattern"
                        title={patternLabel}
                        aria-label={patternLabel}
                      >
                        {header.primarySummary || block.name}
                      </span>
                      {firstFile && (
                        <button
                          type="button"
                          className="task-group-item-file-muted search-file-link"
                          title={firstFileOpenLabel}
                          aria-label={firstFileOpenLabel}
                          onClick={(event) => {
                            event.stopPropagation();
                            void openFile(firstFile.path, firstFile.lineStart, undefined, currentCwd);
                          }}
                          onKeyDown={(event) => {
                            event.stopPropagation();
                          }}
                        >
                          {firstFile.path}
                        </button>
                      )}
                      {header.secondarySummary && (
                        <span
                          className="task-group-item-secondary"
                          title={resultSummaryLabel}
                          aria-label={resultSummaryLabel}
                        >
                          {header.secondarySummary}
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
                      {summary.files.length > 0 && (
                        <div className="search-result-files">
                          {summary.files.map((file) => {
                            const openFileLabel = formatSearchResultLabel(file);

                            return (
                              <button
                                key={`${file.path}:${file.lineStart ?? ''}`}
                                type="button"
                                className="search-result-file-row"
                                title={openFileLabel}
                                aria-label={openFileLabel}
                                onClick={(event) => {
                                  event.stopPropagation();
                                  void openFile(file.path, file.lineStart, undefined, currentCwd);
                                }}
                                onKeyDown={(event) => {
                                  event.stopPropagation();
                                }}
                              >
                                <span className="search-result-file-path">{file.path}</span>
                                {file.lineStart && (
                                  <span
                                    className="search-result-file-line"
                                    title={`Search result line: ${file.path} · L${file.lineStart}`}
                                    aria-label={`Search result line: ${file.path} · L${file.lineStart}`}
                                  >
                                    L{file.lineStart}
                                  </span>
                                )}
                                {file.snippet && (
                                  <span
                                    className="search-result-file-snippet"
                                    title={file.snippet}
                                    aria-label={`Search result snippet: ${file.snippet}`}
                                  >
                                    {file.snippet}
                                  </span>
                                )}
                              </button>
                            );
                          })}
                          {omittedResultCount > 0 && (
                            <div
                              className="search-result-files-footer"
                              title={omittedResultLabel}
                              aria-label={omittedResultLabel}
                            >
                              {omittedResultLabel}
                            </div>
                          )}
                        </div>
                      )}
                      <GenericToolBlock
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

export default SearchToolGroupBlock;
