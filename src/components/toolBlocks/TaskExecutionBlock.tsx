// TaskExecutionBlock - Task 工具（spawn_agent、task）专用块

import {type KeyboardEvent, memo, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {MessageSquare, Wrench} from 'lucide-react';
import type {ToolResultBlock} from '../../types/chat';
import type {ToolInput} from '../../types/tools';
import {useIsToolDenied} from '../../hooks/useIsToolDenied';
import {
    extractAgentToolMeta,
    getAgentToolExtraParams,
    getToolDisplayStatus,
    summarizeAgentToolHeader,
} from '../../utils/toolPresentation';
import {isToolBlockToggleActivationKey} from '../../utils/toolGrouping';
import SubagentHistoryPanel from './SubagentHistoryPanel';

export interface TaskExecutionBlockProps {
  name?: string;
  input?: ToolInput;
  result?: ToolResultBlock | null;
  toolId?: string;
  compact?: boolean;
}

const TaskExecutionBlock = memo(function TaskExecutionBlock({
  name,
  input,
  result,
  toolId,
  compact = false,
}: TaskExecutionBlockProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const isDenied = useIsToolDenied(toolId);

  if (!input) {
    return null;
  }

  const meta = extractAgentToolMeta(input, result);
  const header = summarizeAgentToolHeader(meta, result, 'task');
  const status = getToolDisplayStatus(result, isDenied);
  const extraParams = getAgentToolExtraParams(input);
  const toggleTarget = header.primarySummary || header.secondarySummary || meta.agentId || meta.nickname || name || t('tools.task');
  const toggleLabel = t('tools.taskDetailsToggle', { target: toggleTarget });
  const toggleExpanded = () => setExpanded((prev) => !prev);
  const handleHeaderKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!isToolBlockToggleActivationKey(event.key)) {
      return;
    }

    event.preventDefault();
    toggleExpanded();
  };

  return (
    <div className={`task-container ${compact ? 'task-container-compact' : ''}`}>
      <div
        className={compact ? 'task-header task-header-compact' : 'task-header'}
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        aria-label={toggleLabel}
        title={toggleLabel}
        onClick={toggleExpanded}
        onKeyDown={handleHeaderKeyDown}
        style={{ cursor: 'pointer' }}
      >
        <div className="task-title-section">
          <Wrench className="tool-title-lucide" aria-hidden="true" />
          <span className="tool-title-text">{name || t('tools.task')}</span>
          <span className="tool-command-chip tool-command-run">
            {t('tools.task')}
          </span>
          {header.primarySummary && !expanded && (
            <span
              className="tool-title-summary task-summary-text"
              title={header.primarySummary}
              aria-label={header.primarySummary}
            >
              {header.primarySummary}
            </span>
          )}
          {header.secondarySummary && (
            <span
              className="tool-title-secondary-summary"
              title={header.secondarySummary}
              aria-label={header.secondarySummary}
            >
              {header.secondarySummary}
            </span>
          )}
          {header.runtimeSummary && (
            <span
              className="tool-title-summary tool-title-runtime-summary"
              title={header.runtimeSummary}
              aria-label={header.runtimeSummary}
            >
              {header.runtimeSummary}
            </span>
          )}
          {isDenied && <span className="tool-title-summary text-error">• {t('tools.denied')}</span>}
        </div>
        <div className={`tool-status-indicator ${status}`} />
      </div>

      {expanded && (
        <div className={`task-details ${compact ? 'task-details-compact' : ''}`}>
          <div className="task-content-wrapper">
            {meta.nickname && (
              <div className="tool-section">
                <div className="tool-section-label">{t('tools.nickname')}:</div>
                <div className="task-field-content">{meta.nickname}</div>
              </div>
            )}

            {meta.model && (
              <div className="tool-section">
                <div className="tool-section-label">{t('tools.model')}:</div>
                <div className="task-field-content">{meta.model}</div>
              </div>
            )}

            {meta.reasoningEffort && (
              <div className="tool-section">
                <div className="tool-section-label">{t('tools.reasoningEffort')}:</div>
                <div className="task-field-content">{meta.reasoningEffort}</div>
              </div>
            )}

            {meta.agentId && (
              <div className="tool-section">
                <div className="tool-section-label">{t('tools.agentId')}:</div>
                <div className="task-field-content">
                  <code>{meta.agentId}</code>
                </div>
              </div>
            )}

            {meta.description && (
              <div className="tool-section">
                <div className="tool-section-label">{t('tools.description')}:</div>
                <div className="task-field-content">{meta.description}</div>
              </div>
            )}

            {meta.prompt && (
              <div className="tool-section">
                <div className="tool-section-label">
                  <MessageSquare className="tool-section-label-icon" aria-hidden="true" />
                  {t('tools.prompt')}:
                </div>
                <div className="task-field-content task-prompt">{meta.prompt}</div>
              </div>
            )}

            {extraParams.length > 0 && (
              <div className="tool-section">
                <div className="tool-section-label">{t('tools.inputParameters')}:</div>
                <div className="tool-params">
                  {extraParams.map(([key, value]) => (
                    <div key={key} className="tool-param-row">
                      <span className="tool-param-key">{key}:</span>
                      <span className="tool-param-value">
                        {typeof value === 'object' && value !== null
                          ? JSON.stringify(value, null, 2)
                          : String(value)}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            <SubagentHistoryPanel
              agentId={meta.agentId}
              description={meta.description}
              enabled={expanded}
              hasVisibleMeta={Boolean(
                meta.description
                || meta.prompt
                || meta.agentId
                || meta.model
                || meta.reasoningEffort
                || meta.nickname,
              )}
              result={result}
            />
          </div>
        </div>
      )}
    </div>
  );
});

export default TaskExecutionBlock;
