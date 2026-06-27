import {useTranslation} from 'react-i18next';
import type {SyntheticEvent} from 'react';
import type {DiffPreviewLine} from '../../utils/toolPresentation';

export type EditDiffPreviewMode = 'unified' | 'split';

interface EditDiffPreviewProps {
  id?: string;
  filePath: string;
  additions: number;
  deletions: number;
  lines: DiffPreviewLine[];
  mode?: EditDiffPreviewMode;
  wrapLines?: boolean;
  visible?: boolean;
  floatingTop?: number;
  variant?: 'hover' | 'panel';
  surface?: 'default' | 'status';
  lineLimit?: number;
}

function renderLineNumber(line: DiffPreviewLine): string {
  if (line.kind === 'added') return line.newLineNumber ? String(line.newLineNumber) : '';
  if (line.kind === 'removed') return line.oldLineNumber ? String(line.oldLineNumber) : '';

  if (line.oldLineNumber && line.newLineNumber && line.oldLineNumber !== line.newLineNumber) {
    return `${line.oldLineNumber}/${line.newLineNumber}`;
  }

  return line.newLineNumber || line.oldLineNumber ? String(line.newLineNumber ?? line.oldLineNumber) : '';
}

function renderMarker(line: DiffPreviewLine): string {
  if (line.kind === 'added') return '+';
  if (line.kind === 'removed') return '-';
  return ' ';
}

function renderOldLineNumber(line: DiffPreviewLine): string {
  return line.oldLineNumber ? String(line.oldLineNumber) : '';
}

function renderNewLineNumber(line: DiffPreviewLine): string {
  return line.newLineNumber ? String(line.newLineNumber) : '';
}

function hasUnresolvedTemplate(value: string): boolean {
  return value.includes('{{');
}

export default function EditDiffPreview({
  id,
  filePath,
  additions,
  deletions,
  lines,
  mode = 'unified',
  wrapLines,
  visible = false,
  floatingTop,
  variant = 'hover',
  surface = 'default',
  lineLimit,
}: EditDiffPreviewProps) {
  const { t } = useTranslation();
  const totalLineChange = additions + deletions;
  const fallbackPreviewLabel = totalLineChange > 0
    ? `${additions} added, ${deletions} removed`
    : 'No line delta';
  const translatedPreviewLabel = totalLineChange > 0
    ? t('tools.diffPreviewSummary', {
        defaultValue: fallbackPreviewLabel,
        additions,
        deletions,
      })
    : t('tools.diffPreviewSummaryNoChange', {
        defaultValue: fallbackPreviewLabel,
      });
  const previewLabel = translatedPreviewLabel.includes('{{additions}}') || translatedPreviewLabel.includes('{{deletions}}')
    ? fallbackPreviewLabel
    : translatedPreviewLabel;

  if (lines.length === 0) return null;

  const isPanel = variant === 'panel';
  const isScrollableHover = !isPanel && surface !== 'status';
  const shouldWrapLines = isPanel ? (wrapLines ?? true) : false;
  const defaultHoverLineLimit = surface === 'status' ? 24 : undefined;
  const effectiveLineLimit = lineLimit ?? (isPanel ? undefined : defaultHoverLineLimit);
  const previewLines = typeof effectiveLineLimit === 'number' ? lines.slice(0, effectiveLineLimit) : lines;
  const hiddenLineCount = Math.max(0, lines.length - previewLines.length);
  const fallbackMoreLinesLabel = `${hiddenLineCount} more lines`;
  const translatedMoreLinesLabel = hiddenLineCount > 0
    ? t('tools.moreDiffLines', {
        count: hiddenLineCount,
        defaultValue: fallbackMoreLinesLabel,
      })
    : '';
  const moreLinesLabel = translatedMoreLinesLabel === 'tools.moreDiffLines' || translatedMoreLinesLabel.includes('{{count}}')
    ? fallbackMoreLinesLabel
    : translatedMoreLinesLabel;
  const blankLineLabel = (() => {
    const fallback = 'blank line';
    const translated = t('tools.diffPreviewBlankLine', { defaultValue: fallback });
    return translated === 'tools.diffPreviewBlankLine' || hasUnresolvedTemplate(translated)
      ? fallback
      : translated;
  })();
  const getLineAriaLabel = (line: DiffPreviewLine): string => {
    const lineNumber = renderLineNumber(line);
    const lineText = line.text || blankLineLabel;
    const labelKey = line.kind === 'added'
      ? 'tools.diffPreviewAddedLine'
      : line.kind === 'removed'
        ? 'tools.diffPreviewRemovedLine'
        : 'tools.diffPreviewContextLine';
    const fallbackPrefix = line.kind === 'added'
      ? 'Added'
      : line.kind === 'removed'
        ? 'Removed'
        : 'Context';
    const fallbackLineReference = lineNumber ? `line ${lineNumber}` : 'line';
    const fallback = `${fallbackPrefix} ${fallbackLineReference}: ${lineText}`;
    const translated = t(labelKey, {
      defaultValue: fallback,
      line: lineNumber,
      text: lineText,
    });

    return translated === labelKey || hasUnresolvedTemplate(translated)
      ? fallback
      : translated;
  };
  const getSplitCellLabel = (
    lineLabel: string,
    side: 'old' | 'new',
    isEmpty: boolean,
  ): string => {
    const sideLabel = side === 'old' ? 'Old side' : 'New side';
    const labelKey = isEmpty
      ? side === 'old'
        ? 'tools.diffPreviewOldEmptyCell'
        : 'tools.diffPreviewNewEmptyCell'
      : side === 'old'
        ? 'tools.diffPreviewOldCell'
        : 'tools.diffPreviewNewCell';
    const fallback = isEmpty
      ? `${sideLabel}: no content for ${lineLabel}`
      : `${sideLabel}: ${lineLabel}`;
    const translated = t(labelKey, {
      defaultValue: fallback,
      lineLabel,
    });

    return translated === labelKey || hasUnresolvedTemplate(translated)
      ? fallback
      : translated;
  };
  const rootClassName = isPanel
    ? `edit-diff-panel edit-diff-panel-${mode} ${shouldWrapLines ? 'edit-diff-panel-wrap' : 'edit-diff-panel-nowrap'}`
    : `edit-diff-hover-preview edit-diff-hover-preview-${mode}${isScrollableHover ? ' edit-diff-hover-preview-scrollable' : ''}${surface === 'status' ? ' edit-diff-hover-preview-status edit-diff-hover-preview-solid edit-diff-hover-preview-readable edit-diff-hover-preview-wrap edit-diff-hover-preview-tall' : ''}${visible ? ' is-visible' : ''}`;
  const stopScrollableHoverEvent = (event: SyntheticEvent) => {
    if (!isScrollableHover) return;
    event.stopPropagation();
  };

  return (
    <span
      id={id}
      className={rootClassName}
      role="tooltip"
      style={!isPanel && typeof floatingTop === 'number' ? { top: floatingTop } : undefined}
      onClick={stopScrollableHoverEvent}
      onDoubleClick={stopScrollableHoverEvent}
      onMouseDown={stopScrollableHoverEvent}
      onWheel={stopScrollableHoverEvent}
    >
      <span className="edit-diff-hover-header">
        <span className="edit-diff-hover-path" title={filePath} aria-label={filePath}>{filePath}</span>
        <span className="edit-diff-hover-stats">
          <span className="edit-diff-hover-summary" title={previewLabel} aria-label={previewLabel}>{previewLabel}</span>
          <span className="edit-stat-added" aria-hidden="true">+{additions}</span>
          <span className="edit-stat-deleted" aria-hidden="true">-{deletions}</span>
          {surface === 'status' && hiddenLineCount > 0 && (
            <span className="edit-diff-hover-hidden-summary" title={moreLinesLabel} aria-label={moreLinesLabel}>{moreLinesLabel}</span>
          )}
        </span>
      </span>
      <span className="edit-diff-hover-body" role="list">
        {mode === 'split' ? (
          <span className="edit-diff-hover-split">
            {previewLines.map((line, index) => {
              const lineLabel = getLineAriaLabel(line);
              const oldCellLabel = getSplitCellLabel(lineLabel, 'old', line.kind === 'added');
              const newCellLabel = getSplitCellLabel(lineLabel, 'new', line.kind === 'removed');

              return (
                <span
                  key={`${line.kind}-${index}-${line.oldLineNumber ?? ''}-${line.newLineNumber ?? ''}`}
                  className={`edit-diff-hover-split-row ${line.kind}`}
                  title={lineLabel}
                  role="listitem"
                  aria-label={lineLabel}
                >
                  <span
                    className={`edit-diff-hover-split-cell old ${line.kind === 'added' ? 'empty' : line.kind}`}
                    title={oldCellLabel}
                    aria-label={oldCellLabel}
                  >
                    <span className="edit-diff-hover-number">{line.kind === 'added' ? '' : renderOldLineNumber(line)}</span>
                    <span className="edit-diff-hover-marker">{line.kind === 'added' ? '' : renderMarker(line)}</span>
                    <span className="edit-diff-hover-content">{line.kind === 'added' ? ' ' : (line.text || ' ')}</span>
                  </span>
                  <span
                    className={`edit-diff-hover-split-cell new ${line.kind === 'removed' ? 'empty' : line.kind}`}
                    title={newCellLabel}
                    aria-label={newCellLabel}
                  >
                    <span className="edit-diff-hover-number">{line.kind === 'removed' ? '' : renderNewLineNumber(line)}</span>
                    <span className="edit-diff-hover-marker">{line.kind === 'removed' ? '' : renderMarker(line)}</span>
                    <span className="edit-diff-hover-content">{line.kind === 'removed' ? ' ' : (line.text || ' ')}</span>
                  </span>
                </span>
              );
            })}
          </span>
        ) : (
          previewLines.map((line, index) => {
            const lineLabel = getLineAriaLabel(line);

            return (
              <span
                key={`${line.kind}-${index}-${line.oldLineNumber ?? ''}-${line.newLineNumber ?? ''}`}
                className={`edit-diff-hover-line ${line.kind}`}
                title={lineLabel}
                role="listitem"
                aria-label={lineLabel}
              >
                <span className="edit-diff-hover-number">{renderLineNumber(line)}</span>
                <span className="edit-diff-hover-marker">{renderMarker(line)}</span>
                <span className="edit-diff-hover-content">{line.text || ' '}</span>
              </span>
            );
          })
        )}
        {hiddenLineCount > 0 && surface !== 'status' && (
          <span className="edit-diff-hover-more" title={moreLinesLabel} aria-label={moreLinesLabel}>
            {moreLinesLabel}
          </span>
        )}
      </span>
    </span>
  );
}
