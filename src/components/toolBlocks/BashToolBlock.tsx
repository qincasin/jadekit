// BashToolBlock - Bash 命令执行工具块

import {memo, useRef, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {Terminal} from 'lucide-react';
import type {ToolResultBlock} from '../../types/chat';
import type {ToolInput} from '../../types/tools';
import {useIsToolDenied} from '../../hooks/useIsToolDenied';
import {
    extractResultText,
    summarizeBashHeaderResult,
    summarizeCommand,
    truncateContent,
} from '../../utils/toolPresentation';
import {copyToClipboard} from '../../utils/bridge';
import {isToolBlockToggleActivationKey} from '../../utils/toolGrouping';

export interface BashToolBlockProps {
  name?: string;
  input?: ToolInput;
  result?: ToolResultBlock | null;
  toolId?: string;
  compact?: boolean;
}

interface BashResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}

type BashCopiedTarget = 'command' | 'output' | null;

/**
 * 解析 Bash 工具结果
 */
function parseBashResult(result: ToolResultBlock): BashResult {
  const text = extractResultText(result);

  // 尝试解析 JSON 格式
  try {
    const parsed = JSON.parse(text);
    return {
      exitCode: parsed.exit_code ?? parsed.exitCode ?? (result.is_error ? 1 : 0),
      stdout: parsed.stdout ?? '',
      stderr: parsed.stderr ?? '',
    };
  } catch {
    // 纯文本输出
    return {
      exitCode: result.is_error ? 1 : 0,
      stdout: text,
      stderr: '',
    };
  }
}

const BashToolBlock = memo(function BashToolBlock({
  name: _name,
  input,
  result,
  toolId,
  compact = false,
}: BashToolBlockProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [copiedTarget, setCopiedTarget] = useState<BashCopiedTarget>(null);
  const copiedResetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isDenied = useIsToolDenied(toolId);

  if (!input) {
    return null;
  }

  // 提取命令
  const command = (input.command as string) || '';
  const commandSummary = summarizeCommand(command);
  const workdir = typeof input.workdir === 'string' ? input.workdir : '';

  // 状态计算
  const isCompleted = (result !== undefined && result !== null) || isDenied;
  const isError = isDenied || (isCompleted && result?.is_error === true);
  const status = isError ? 'error' : isCompleted ? 'completed' : 'pending';

  // 解析结果
  const bashResult = result ? parseBashResult(result) : null;
  const resultSummary = bashResult ? summarizeBashHeaderResult(bashResult) : '';
  const headerToggleTarget = commandSummary.summary || commandSummary.label || t('tools.runCommand');
  const headerToggleLabel = t('tools.bashDetailsToggle', { target: headerToggleTarget });
  const copyCommandButtonLabel = t('tools.copyCommand');
  const copyOutputButtonLabel = t('tools.copyOutput');
  const copyCommandActionLabel = t('tools.copyCommandForCommand', { target: headerToggleTarget });
  const copyOutputActionLabel = t('tools.copyOutputForCommand', { target: headerToggleTarget });

  const markCopied = (target: Exclude<BashCopiedTarget, null>) => {
    if (copiedResetTimerRef.current) {
      clearTimeout(copiedResetTimerRef.current);
    }
    setCopiedTarget(target);
    copiedResetTimerRef.current = setTimeout(() => {
      setCopiedTarget(null);
      copiedResetTimerRef.current = null;
    }, 2000);
  };

  // 复制功能
  const handleCopyCommand = async (event?: React.MouseEvent) => {
    event?.stopPropagation();
    await copyToClipboard(command);
    markCopied('command');
  };

  const handleCopyOutput = async (event?: React.MouseEvent) => {
    event?.stopPropagation();
    if (bashResult) {
      const output = bashResult.stdout + (bashResult.stderr ? `\n\nStderr:\n${bashResult.stderr}` : '');
      await copyToClipboard(output);
      markCopied('output');
    }
  };

  const toggleExpanded = () => setExpanded((prev) => !prev);

  const handleHeaderKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!isToolBlockToggleActivationKey(event.key)) return;
    event.preventDefault();
    toggleExpanded();
  };

  const detailContent = (
    <div className="task-content-wrapper">
      <div className="tool-section">
        <div className="tool-section-label">{t('tools.command')}:</div>
        <div className="tool-command-meta">
          <span className={`tool-command-chip ${commandSummary.accentClass}`}>
            {commandSummary.label}
          </span>
          {workdir && (
            <span className="tool-command-workdir">
              {workdir}
            </span>
          )}
        </div>
        <div className="bash-command-block">
          <code>{command}</code>
        </div>
      </div>

      {bashResult && (
        <div className="tool-section">
          <div className="tool-section-label">{t('tools.exitCode')}:</div>
          <div className={`bash-exit-code ${bashResult.exitCode === 0 ? 'success' : 'error'}`}>
            {bashResult.exitCode}
            {bashResult.exitCode === 0 ? ` (${t('tools.success')})` : ` (${t('tools.failed')})`}
          </div>
        </div>
      )}

      {bashResult && bashResult.stdout && (
        <div className="tool-section">
          <div className="tool-section-label">{t('tools.result')}:</div>
          <div className="bash-output">
            <pre className="bash-output-text">{truncateContent(bashResult.stdout, 10000)}</pre>
          </div>
        </div>
      )}

      {bashResult && bashResult.stderr && (
        <div className="tool-section">
          <div className="tool-section-label">{t('tools.errorOutput')}:</div>
          <div className="bash-output bash-output-error">
            <pre className="bash-output-text">{truncateContent(bashResult.stderr, 10000)}</pre>
          </div>
        </div>
      )}

      <div className="tool-actions">
        <button
          type="button"
          className={`btn btn-sm ${copiedTarget === 'command' ? 'btn-success' : 'btn-ghost'}`}
          title={copyCommandActionLabel}
          aria-label={copyCommandActionLabel}
          onClick={handleCopyCommand}
        >
          {copiedTarget === 'command' ? t('tools.copied') : copyCommandButtonLabel}
        </button>
        {bashResult && (
          <button
            type="button"
            className={`btn btn-sm ${copiedTarget === 'output' ? 'btn-success' : 'btn-ghost'}`}
            title={copyOutputActionLabel}
            aria-label={copyOutputActionLabel}
            onClick={handleCopyOutput}
          >
            {copiedTarget === 'output' ? t('tools.copied') : copyOutputButtonLabel}
          </button>
        )}
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
          <Terminal className="tool-title-lucide" aria-hidden="true" />
          <span className="tool-title-text">{t('tools.runCommand')}</span>
          <span className={`tool-command-chip ${commandSummary.accentClass}`}>
            {commandSummary.label}
          </span>
          <span className="tool-title-summary bash-command" title={command} aria-label={command}>
            {commandSummary.summary}
          </span>
          {resultSummary && (
            <span
              className={[
                'tool-title-secondary-summary',
                isError ? 'tool-title-secondary-summary-error' : '',
              ].filter(Boolean).join(' ')}
              title={resultSummary}
              aria-label={resultSummary}
            >
              {resultSummary}
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

export default BashToolBlock;
