// Tauri 桥接函数

import {invoke} from '@tauri-apps/api/core';
import i18n from '../i18n';
import {showToast} from '../components/common/ToastContainer';

const CONTROL_CHAR_REGEX = /[\u0000-\u001f]/;
const LITERAL_PERCENT_REGEX = /%(?![0-9A-Fa-f]{2})/g;

function stripWrappingQuotes(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length >= 2) {
    const first = trimmed[0];
    const last = trimmed[trimmed.length - 1];
    if ((first === '"' && last === '"') || (first === '\'' && last === '\'')) {
      return trimmed.slice(1, -1);
    }
  }
  return trimmed;
}

function stripFileUrlPrefix(value: string): string {
  if (value.startsWith('file:///')) {
    const rest = value.slice('file:///'.length);
    return /^[A-Za-z]:[\\/]/.test(rest) ? rest : `/${rest}`;
  }
  if (value.startsWith('file://')) {
    return value.slice('file://'.length);
  }
  return value;
}

function isValidOpenFileTarget(value: string): boolean {
  return value.length > 0 && !CONTROL_CHAR_REGEX.test(value);
}

function decodeOpenFileTarget(value: string): string | null {
  let current = value;
  for (let pass = 0; pass < 3; pass += 1) {
    try {
      const decoded = decodeURIComponent(current.replace(LITERAL_PERCENT_REGEX, '%25'));
      if (decoded === current) {
        return decoded;
      }
      current = decoded;
    } catch {
      return null;
    }
  }
  return current;
}

function normalizeOpenFileTarget(filePath: string): string | null {
  const stripped = stripWrappingQuotes(filePath);
  const decoded = decodeOpenFileTarget(stripped);
  if (!decoded) return null;

  const normalized = stripFileUrlPrefix(decoded);
  return isValidOpenFileTarget(normalized) ? normalized : null;
}

function isDecimal(value: string): boolean {
  return value.length > 0 && /^\d+$/.test(value);
}

function splitEditorLineSuffix(filePath: string): {
  path: string;
  lineStart?: number;
  lineEnd?: number;
} {
  const lastColon = filePath.lastIndexOf(':');
  if (lastColon < 0) return {path: filePath};

  const tail = filePath.slice(lastColon + 1);
  const rangeParts = tail.split('-');
  if (
    rangeParts.length === 2
    && isDecimal(rangeParts[0])
    && isDecimal(rangeParts[1])
  ) {
    const lineStart = Number.parseInt(rangeParts[0], 10);
    const lineEnd = Number.parseInt(rangeParts[1], 10);
    if (lineStart > 0 && lineEnd >= lineStart) {
      return {path: filePath.slice(0, lastColon), lineStart, lineEnd};
    }
  }

  if (!isDecimal(tail)) return {path: filePath};

  const beforeLastColon = filePath.slice(0, lastColon);
  const previousColon = beforeLastColon.lastIndexOf(':');
  if (previousColon >= 0) {
    const possibleLine = beforeLastColon.slice(previousColon + 1);
    if (isDecimal(possibleLine)) {
      const lineStart = Number.parseInt(possibleLine, 10);
      return lineStart > 0
        ? {path: beforeLastColon.slice(0, previousColon), lineStart}
        : {path: filePath};
    }
  }

  const lineStart = Number.parseInt(tail, 10);
  return lineStart > 0 ? {path: beforeLastColon, lineStart} : {path: filePath};
}

/**
 * 在编辑器中打开文件
 * @param filePath 文件路径
 * @param lineStart 起始行号（可选）
 * @param lineEnd 结束行号（可选）
 */
export async function openFile(
  filePath: string,
  lineStart?: number,
  lineEnd?: number,
  cwd?: string | null
): Promise<boolean> {
  const normalizedTarget = normalizeOpenFileTarget(filePath);
  if (!normalizedTarget) return false;

  const parsedTarget = splitEditorLineSuffix(normalizedTarget);
  const resolvedLineStart = lineStart ?? parsedTarget.lineStart;
  const resolvedLineEnd = lineEnd ?? parsedTarget.lineEnd;

  try {
    await invoke('open_file_in_editor', {
      filePath: parsedTarget.path,
      lineStart: resolvedLineStart,
      lineEnd: resolvedLineEnd,
      cwd: cwd || undefined,
    });
    return true;
  } catch (error) {
    console.error('Failed to open file:', error);
    showToast(`${i18n.t('tools.openFileFailed')}: ${String(error)}`, 'error', 5000);
    return false;
  }
}

/**
 * 复制文本到剪贴板
 * @param text 要复制的文本
 */
export async function copyToClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch (error) {
    console.error('Failed to copy to clipboard:', error);
    // 降级方案：使用传统方式
    const textArea = document.createElement('textarea');
    textArea.value = text;
    textArea.style.position = 'fixed';
    textArea.style.left = '-999999px';
    textArea.style.top = '-999999px';
    document.body.appendChild(textArea);
    textArea.focus();
    textArea.select();
    try {
      document.execCommand('copy');
    } catch (err) {
      console.error('Fallback: Failed to copy', err);
    }
    document.body.removeChild(textArea);
  }
}
