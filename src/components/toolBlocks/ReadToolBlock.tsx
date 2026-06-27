// ReadToolBlock - 文件读取工具块

import {memo, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {FileSearch} from 'lucide-react';
import type {ToolResultBlock} from '../../types/chat';
import type {ToolInput} from '../../types/tools';
import {useIsToolDenied} from '../../hooks/useIsToolDenied';
import {useChatStore} from '../../stores/useChatStore';
import {formatLineRange, getToolDisplayStatus, getToolLineInfo, resolveToolTarget} from '../../utils/toolPresentation';
import {getFileIcon, getFolderIcon} from '../../utils/fileIcons';
import {copyToClipboard, openFile} from '../../utils/bridge';
import {isToolBlockToggleActivationKey} from '../../utils/toolGrouping';

export interface ReadToolBlockProps {
  name?: string;
  input?: ToolInput;
  result?: ToolResultBlock | null;
  toolId?: string;
  compact?: boolean;
}

const ReadToolBlock = memo(function ReadToolBlock({
  name: _name,
  input,
  result,
  toolId,
  compact = false,
}: ReadToolBlockProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const isDenied = useIsToolDenied(toolId);
  const currentCwd = useChatStore((state) => state.currentCwd);

  if (!input) {
    return null;
  }

  // 解析文件路径
  const target = resolveToolTarget(input);
  const lineInfo = getToolLineInfo(input, target);
  const filePath = target?.rawPath || '';
  const lineRangeLabel = formatLineRange(lineInfo);

  // 状态计算
  const status = getToolDisplayStatus(result, isDenied);
  const copyPathButtonLabel = t('tools.copyPath');
  const copyPathActionLabel = t('tools.copyPathForPath', { file: target?.displayPath || filePath });
  const openFileLabel = target?.isFile ? `${t('tools.openFile')}: ${target.displayPath || filePath}` : '';
  const headerToggleTarget = target?.displayPath || filePath || t('tools.read');
  const headerToggleLabel = t('tools.readDetailsToggle', { target: headerToggleTarget });

  // 文件图标
  const fileIconSvg = target
    ? target.isDirectory
      ? getFolderIcon(target.cleanFileName)
      : getFileIcon(target.cleanFileName.split('.').pop() || '', target.cleanFileName)
    : '';

  // 文件路径点击
  const handleFileClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (target?.isFile) {
      void openFile(target.openPath, lineInfo.start, lineInfo.end, currentCwd);
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
      <div className="tool-section">
        <div className="tool-section-label">{t('tools.filePath')}:</div>
        <div className="file-path-display">
          <code>{filePath}</code>
        </div>
      </div>

      {lineInfo.start && (
        <div className="tool-section">
          <div className="tool-section-label">{t('tools.lines')}:</div>
          <div className="line-range-display">
            {lineInfo.end && lineInfo.end !== lineInfo.start
              ? `${lineInfo.start} - ${lineInfo.end}`
              : lineInfo.start}
          </div>
        </div>
      )}

      {Object.entries(input)
        .filter(([key]) => !['file_path', 'path', 'target_file', 'offset', 'limit', 'line', 'start_line', 'end_line', 'command', 'workdir', 'description'].includes(key))
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

      <div className="tool-actions">
        {target?.isFile && (
          <button
            type="button"
            className="btn btn-sm btn-ghost"
            title={openFileLabel}
            aria-label={openFileLabel}
            onClick={(event) => {
              event.stopPropagation();
              void openFile(target.openPath, lineInfo.start, lineInfo.end, currentCwd);
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
          <FileSearch className="tool-title-lucide" aria-hidden="true" />
          <span className="tool-title-text">{t('tools.read')}</span>
          {target?.isFile ? (
            <button
              type="button"
              className="tool-title-summary file-path-link file-path-button clickable-file"
              title={openFileLabel}
              aria-label={openFileLabel}
              onClick={handleFileClick}
              onKeyDown={(event) => {
                event.stopPropagation();
              }}
            >
              {fileIconSvg && (
                <span
                  className="file-icon"
                  dangerouslySetInnerHTML={{ __html: fileIconSvg }}
                />
              )}
              {target.displayPath || filePath}
            </button>
          ) : (
            <span
              className="tool-title-summary file-path-link"
              title={filePath}
              aria-label={filePath}
            >
              {fileIconSvg && (
                <span
                  className="file-icon"
                  dangerouslySetInnerHTML={{ __html: fileIconSvg }}
                />
              )}
              {target?.displayPath || filePath}
            </span>
          )}
          {lineRangeLabel && (
            <span
              className="tool-title-summary line-info"
              title={lineRangeLabel}
              aria-label={lineRangeLabel}
            >
              {lineRangeLabel}
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

export default ReadToolBlock;
