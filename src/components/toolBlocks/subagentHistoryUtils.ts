import type {ChatMessage, ContentBlock, ToolResultBlock} from '../../types/chat';
import type {ToolInput} from '../../types/tools';
import {getContentBlocksFromRaw} from '../../utils/chatMessageFlow';
import {
    extractResultText,
    getToolLineInfo,
    resolveToolTarget,
    summarizeCommand,
    summarizeGroupBashItemResult,
    summarizeSearchResultText,
    summarizeToolResultText,
} from '../../utils/toolPresentation';

interface ResolveSubagentHistoryRequestParams {
  sessionId: string | null;
  sourcePath: string | null;
  currentCwd: string | null;
  agentId?: string | null;
  description?: string | null;
}

export interface ResolvedSubagentHistoryRequest {
  requestSessionId: string | null;
  requestSourcePath: string | null;
  hasAgentIdentity: boolean;
  canLoad: boolean;
}

export interface SubagentProcessToolCall {
  id: string;
  name: string;
  detail?: string;
  target?: SubagentProcessFile;
  resultSummary?: string;
  resultFile?: SubagentProcessFile;
  category: 'read' | 'tool';
}

export interface SubagentToolPresentation {
  label: string;
  accentClass: string;
  summary: string;
  iconKind: 'command' | 'search' | 'read' | 'list' | 'patch' | 'web' | 'agent' | 'default';
}

export interface SubagentProcessFile {
  id: string;
  displayPath: string;
  openPath: string;
  lineStart?: number;
  lineEnd?: number;
}

export interface SubagentProcessModel {
  thought: string;
  readFiles: SubagentProcessFile[];
  toolCalls: SubagentProcessToolCall[];
  finalSummary: string;
  toolUseCount: number;
  stepCount: number;
  totalDurationMs?: number;
  totalTokens?: number;
  totalToolUseCount?: number;
  fullResultText?: string;
}

interface SubagentResultRuntimeMeta {
  totalDurationMs?: number;
  totalTokens?: number;
  totalToolUseCount?: number;
  fullResultText?: string;
  summaryText?: string;
}

function deriveSessionIdFromSourcePath(sourcePath: string | null): string | null {
  if (!sourcePath) {
    return null;
  }
  const normalized = sourcePath.replace(/\\/g, '/');
  const fileName = normalized.split('/').pop() ?? '';
  if (!fileName.endsWith('.jsonl')) {
    return null;
  }
  return fileName.slice(0, -'.jsonl'.length) || null;
}

function normalizeOptionalText(value?: string | null): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

function compactPath(path: string): string {
  const normalized = path.replace(/\\/g, '/');
  const parts = normalized.split('/').filter(Boolean);
  return parts.length > 4 ? `…/${parts.slice(-4).join('/')}` : normalized;
}

function getToolFilePath(block: Extract<ContentBlock, { type: 'tool_use' }>): string | undefined {
  const input = block.input;
  if (!input || typeof input !== 'object') {
    return undefined;
  }
  const filePath = input.file_path ?? input.filePath ?? input.path;
  return typeof filePath === 'string' && filePath.trim() ? filePath.trim() : undefined;
}

function getToolDetail(block: Extract<ContentBlock, { type: 'tool_use' }>): string | undefined {
  const input = block.input;
  if (!input || typeof input !== 'object') {
    return undefined;
  }
  const command = input.command ?? input.cmd;
  if (typeof command === 'string' && command.trim()) {
    return command.trim();
  }
  const url = input.url ?? input.href;
  if (typeof url === 'string' && url.trim()) {
    return url.trim();
  }
  const pattern = input.pattern ?? input.query;
  if (typeof pattern === 'string' && pattern.trim()) {
    return pattern.trim();
  }
  return undefined;
}

function firstNonEmptyLine(text: string): string {
  return text
    .split('\n')
    .map((line) => line.trim())
    .find(Boolean) ?? '';
}

function formatSearchResultSummary(matchCount: number, fileCount: number): string {
  const matchLabel = matchCount === 1 ? 'match' : 'matches';
  const fileLabel = fileCount === 1 ? 'file' : 'files';
  return `${matchCount} ${matchLabel} in ${fileCount} ${fileLabel}`;
}

function formatFileCountSummary(fileCount: number): string {
  return `${fileCount} ${fileCount === 1 ? 'file' : 'files'}`;
}

function formatWebResultSummary(resultText: string): string {
  try {
    const parsed = JSON.parse(resultText) as Record<string, unknown>;
    const candidates = [
      parsed.title,
      parsed.name,
      parsed.url,
      parsed.content,
      parsed.summary,
      parsed.description,
    ];
    for (const candidate of candidates) {
      if (typeof candidate === 'string' && candidate.trim()) {
        return summarizeToolResultText(candidate.trim(), 72) || candidate.trim();
      }
    }
  } catch {
    // fallback below
  }

  return summarizeToolResultText(resultText, 72);
}

function formatGenericJsonResultSummary(resultText: string): string {
  try {
    const parsed = JSON.parse(resultText) as Record<string, unknown>;
    const candidates = [
      parsed.message,
      parsed.title,
      parsed.name,
      parsed.summary,
      parsed.description,
      parsed.result,
      parsed.output,
      parsed.stdout,
      parsed.content,
    ];
    for (const candidate of candidates) {
      if (typeof candidate === 'string' && candidate.trim()) {
        return summarizeToolResultText(candidate.trim(), 72) || candidate.trim();
      }
    }
  } catch {
    // fallback below
  }

  return summarizeToolResultText(resultText, 72);
}

function normalizeToolName(value: string): string {
  return value.toLowerCase().replace(/[\s_-]+/g, '');
}

function summarizeToolLabel(label: string): string {
  return label.trim() || 'Tool';
}

function toSubagentToolTarget(rawPath: string, lineStart?: number, lineEnd?: number): SubagentProcessFile {
  return {
    id: lineStart
      ? `${rawPath}:${lineStart}${lineEnd && lineEnd !== lineStart ? `-${lineEnd}` : ''}`
      : rawPath,
    openPath: rawPath,
    displayPath: compactPath(rawPath),
    ...(lineStart ? { lineStart } : {}),
    ...(lineEnd ? { lineEnd } : {}),
  };
}

export function summarizeSubagentProcessToolCall(tool: SubagentProcessToolCall): SubagentToolPresentation {
  const normalizedName = normalizeToolName(tool.name);
  const detail = tool.detail?.trim() ?? '';
  const fallbackSummary = detail || summarizeToolLabel(tool.name);

  if (normalizedName.includes('read')) {
    return {
      label: 'Read',
      accentClass: 'tool-command-read',
      summary: fallbackSummary,
      iconKind: 'read',
    };
  }

  if (normalizedName.includes('glob')) {
    return {
      label: 'Glob',
      accentClass: 'tool-command-search',
      summary: fallbackSummary,
      iconKind: 'search',
    };
  }

  if (normalizedName.includes('grep') || normalizedName.includes('search') || normalizedName.includes('find')) {
    return {
      label: 'Search',
      accentClass: 'tool-command-search',
      summary: fallbackSummary,
      iconKind: 'search',
    };
  }

  if (
    normalizedName.includes('list')
    || normalizedName === 'ls'
    || normalizedName === 'dir'
    || normalizedName.includes('getchilditem')
    || normalizedName === 'gci'
  ) {
    return {
      label: 'List',
      accentClass: 'tool-command-list',
      summary: fallbackSummary,
      iconKind: 'list',
    };
  }

  if (normalizedName.includes('patch')) {
    return {
      label: 'Patch',
      accentClass: 'tool-command-patch',
      summary: fallbackSummary,
      iconKind: 'patch',
    };
  }

  if (normalizedName.includes('edit') || normalizedName.includes('write')) {
    return {
      label: 'Edit',
      accentClass: 'tool-command-patch',
      summary: fallbackSummary,
      iconKind: 'patch',
    };
  }

  if (
    normalizedName.includes('bash')
    || normalizedName.includes('shell')
    || normalizedName.includes('run')
    || normalizedName.includes('execute')
    || normalizedName.includes('command')
  ) {
    const commandSummary = summarizeCommand(detail || tool.name);
    if (commandSummary.label !== 'Command' || commandSummary.accentClass !== 'tool-command-default') {
      return {
        label: commandSummary.label,
        accentClass: commandSummary.accentClass,
        summary: commandSummary.summary,
        iconKind: 'command',
      };
    }

    return {
      label: 'Run',
      accentClass: 'tool-command-run',
      summary: fallbackSummary,
      iconKind: 'command',
    };
  }

  if (normalizedName.includes('web') || normalizedName.includes('fetch')) {
    return {
      label: normalizedName.includes('fetch') ? 'Fetch' : 'Web',
      accentClass: 'tool-command-web',
      summary: fallbackSummary,
      iconKind: 'web',
    };
  }

  if (normalizedName.includes('agent') || normalizedName.includes('task') || normalizedName.includes('spawn')) {
    return {
      label: normalizedName.includes('task') ? 'Task' : 'Agent',
      accentClass: 'tool-command-plan',
      summary: fallbackSummary,
      iconKind: 'agent',
    };
  }

  return {
    label: summarizeToolLabel(tool.name),
    accentClass: 'tool-command-default',
    summary: fallbackSummary,
    iconKind: 'default',
  };
}

function pushUniqueFile(
  list: SubagentProcessFile[],
  openPath: string,
  lineStart?: number,
  lineEnd?: number,
) {
  if (list.some((item) => item.openPath === openPath && item.lineStart === lineStart && item.lineEnd === lineEnd)) {
    return;
  }
  const lineSuffix = lineStart ? `:${lineStart}${lineEnd && lineEnd !== lineStart ? `-${lineEnd}` : ''}` : '';
  list.push({
    id: `${openPath}${lineSuffix}`,
    openPath,
    displayPath: compactPath(openPath),
    ...(lineStart ? { lineStart } : {}),
    ...(lineEnd ? { lineEnd } : {}),
  });
}

function numberValue(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === 'string' && /^\d+$/.test(value.trim())) {
    return Number(value.trim());
  }
  return undefined;
}

function parseJsonPrefix(text: string): Record<string, unknown> | null {
  const trimmed = text.trimStart();
  if (!trimmed.startsWith('{')) {
    return null;
  }

  let depth = 0;
  let inString = false;
  let escaped = false;

  for (let index = 0; index < trimmed.length; index += 1) {
    const char = trimmed[index];
    if (escaped) {
      escaped = false;
      continue;
    }
    if (char === '\\') {
      escaped = true;
      continue;
    }
    if (char === '"') {
      inString = !inString;
      continue;
    }
    if (inString) {
      continue;
    }
    if (char === '{') {
      depth += 1;
      continue;
    }
    if (char === '}') {
      depth -= 1;
      if (depth === 0) {
        const candidate = trimmed.slice(0, index + 1);
        try {
          const parsed = JSON.parse(candidate);
          return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
            ? parsed as Record<string, unknown>
            : null;
        } catch {
          return null;
        }
      }
    }
  }

  return null;
}

function stripJsonPrefix(text: string): string {
  const trimmed = text.trimStart();
  if (!trimmed.startsWith('{')) {
    return text.trim();
  }

  let depth = 0;
  let inString = false;
  let escaped = false;

  for (let index = 0; index < trimmed.length; index += 1) {
    const char = trimmed[index];
    if (escaped) {
      escaped = false;
      continue;
    }
    if (char === '\\') {
      escaped = true;
      continue;
    }
    if (char === '"') {
      inString = !inString;
      continue;
    }
    if (inString) {
      continue;
    }
    if (char === '{') {
      depth += 1;
      continue;
    }
    if (char === '}') {
      depth -= 1;
      if (depth === 0) {
        return trimmed.slice(index + 1).trim();
      }
    }
  }

  return text.trim();
}

export function extractSubagentResultRuntimeMeta(result?: ToolResultBlock | null): SubagentResultRuntimeMeta {
  const resultText = result ? extractResultText(result) : '';
  const parsed = parseJsonPrefix(resultText);
  const summaryText = stripJsonPrefix(resultText);

  return {
    totalDurationMs: numberValue(parsed?.totalDurationMs ?? parsed?.durationMs),
    totalTokens: numberValue(parsed?.totalTokens),
    totalToolUseCount: numberValue(parsed?.totalToolUseCount),
    fullResultText: resultText.trim() || undefined,
    summaryText: summaryText || undefined,
  };
}

export function resolveSubagentHistoryRequest(
  params: ResolveSubagentHistoryRequestParams,
): ResolvedSubagentHistoryRequest {
  const requestSessionId = normalizeOptionalText(params.sessionId)
    ?? deriveSessionIdFromSourcePath(params.sourcePath);
  const requestSourcePath = normalizeOptionalText(params.sourcePath)
    ?? normalizeOptionalText(params.currentCwd);
  const hasAgentIdentity = Boolean(
    normalizeOptionalText(params.agentId)
    || normalizeOptionalText(params.description),
  );

  return {
    requestSessionId,
    requestSourcePath,
    hasAgentIdentity,
    canLoad: Boolean(requestSessionId && requestSourcePath && hasAgentIdentity),
  };
}

export function buildSubagentProcessModel(
  messages: ChatMessage[],
  runtimeMeta?: SubagentResultRuntimeMeta,
): SubagentProcessModel {
  const readFiles: SubagentProcessFile[] = [];
  const toolCalls: SubagentProcessToolCall[] = [];
  const resultTextByToolId = new Map<string, string>();
  let thought = '';
  let finalSummary = '';
  let toolUseCount = 0;

  messages.forEach((message) => {
    const blocks = getContentBlocksFromRaw(message.raw);
    blocks.forEach((block) => {
      if (block.type === 'tool_result' && block.tool_use_id) {
        const resultText = extractResultText(block).trim();
        if (resultText) {
          resultTextByToolId.set(block.tool_use_id, resultText);
        }
      }
    });
  });

  messages.forEach((message, index) => {
    const blocks = getContentBlocksFromRaw(message.raw);
    blocks.forEach((block) => {
      if (block.type === 'thinking' && !thought && block.thinking.trim()) {
        thought = firstNonEmptyLine(block.thinking);
        return;
      }

      if (block.type !== 'tool_use') {
        return;
      }

      toolUseCount += 1;
      const normalizedName = normalizeToolName(block.name);
      const rawFilePath = getToolFilePath(block);
      const detail = getToolDetail(block);
      const lineInfo = getToolLineInfo(block.input as ToolInput);
      const target = resolveToolTarget(block.input as ToolInput);
      const resultText = resultTextByToolId.get(block.id);
      const searchResultSummary = resultText ? summarizeSearchResultText(resultText) : undefined;
      const isSearchLikeTool = normalizedName.includes('grep')
        || normalizedName.includes('search')
        || normalizedName.includes('find');
      const isCommandLikeTool = normalizedName.includes('bash')
        || normalizedName.includes('shell')
        || normalizedName.includes('run')
        || normalizedName.includes('execute')
        || normalizedName.includes('command');
      const isWebLikeTool = normalizedName.includes('web')
        || normalizedName.includes('fetch');
      const resultSummary = searchResultSummary && searchResultSummary.files.length > 0
        ? (
            isSearchLikeTool
              ? formatSearchResultSummary(searchResultSummary.matchCount, searchResultSummary.fileCount)
              : formatFileCountSummary(searchResultSummary.fileCount)
          )
        : isCommandLikeTool && resultText
          ? summarizeGroupBashItemResult({
              type: 'tool_result',
              tool_use_id: block.id,
              content: resultText,
            })
        : isWebLikeTool && resultText
          ? formatWebResultSummary(resultText)
        : resultText && resultText.trim().startsWith('{')
          ? formatGenericJsonResultSummary(resultText)
        : resultText
          ? summarizeToolResultText(resultText)
          : undefined;
      const resultSummaryFiles = searchResultSummary?.files ?? [];
      if (block.name.toLowerCase() === 'read' && rawFilePath) {
        pushUniqueFile(readFiles, rawFilePath, lineInfo.start, lineInfo.end);
        return;
      }

      toolCalls.push({
        id: block.id || `${index}-${toolCalls.length}`,
        name: block.name,
        detail,
        ...(target ? { target: toSubagentToolTarget(target.openPath, target.lineStart, target.lineEnd) } : {}),
        ...(resultSummary ? { resultSummary } : {}),
        ...(resultSummaryFiles[0] ? { resultFile: toSubagentToolTarget(resultSummaryFiles[0].path, resultSummaryFiles[0].lineStart) } : {}),
        category: 'tool',
      });
    });

    if (!finalSummary && message.role === 'assistant' && message.content.trim()) {
      finalSummary = firstNonEmptyLine(message.content);
    }
  });

  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.role === 'assistant' && message.content.trim()) {
      finalSummary = firstNonEmptyLine(message.content);
      break;
    }
  }

  return {
    thought,
    readFiles,
    toolCalls,
    finalSummary: finalSummary || summarizeToolResultText(runtimeMeta?.summaryText ?? runtimeMeta?.fullResultText ?? ''),
    toolUseCount: runtimeMeta?.totalToolUseCount ?? toolUseCount,
    stepCount: readFiles.length + toolCalls.length,
    totalDurationMs: runtimeMeta?.totalDurationMs,
    totalTokens: runtimeMeta?.totalTokens,
    totalToolUseCount: runtimeMeta?.totalToolUseCount,
    fullResultText: runtimeMeta?.fullResultText,
  };
}
