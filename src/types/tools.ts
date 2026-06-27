// 工具分类与常量定义

/** 工具输入参数（通用） */
export interface ToolInput {
  [key: string]: unknown;
  // 常见字段
  file_path?: string;
  filePath?: string;
  path?: string;
  target_file?: string;
  targetFile?: string;
  command?: string;
  old_string?: string;
  oldString?: string;
  oldText?: string;
  new_string?: string;
  newString?: string;
  newText?: string;
  edits?: unknown[];
  patch?: string;
  input?: string;
  content?: string;
  offset?: number;
  limit?: number;
  line?: number;
  start_line?: number;
  end_line?: number;
  description?: string;
  prompt?: string;
  model?: string;
  reasoning_effort?: string;
  reasoningEffort?: string;
  subagent_type?: string;
  name?: string;
  agent_id?: string;
  agentId?: string;
}

/** Read 工具名称集合 */
export const READ_TOOL_NAMES = new Set([
  'read',
  'Read',
  'readfile',
  'ReadFile',
  'read_file',
]);

/** Edit 工具名称集合 */
export const EDIT_TOOL_NAMES = new Set([
  'edit',
  'Edit',
  'editfile',
  'edit_file',
  'write',
  'Write',
  'writefile',
  'WriteFile',
  'write_file',
  'writetofile',
  'write_to_file',
  'replace',
  'replacestring',
  'replace_string',
  'multiedit',
  'MultiEdit',
  'notebookedit',
  'NotebookEdit',
  'applypatch',
  'apply_patch',
]);

/** Bash 工具名称集合 */
export const BASH_TOOL_NAMES = new Set([
  'bash',
  'Bash',
  'executecommand',
  'ExecuteCommand',
  'execute_command',
]);

/** Search 工具名称集合 */
export const SEARCH_TOOL_NAMES = new Set([
  'grep',
  'Grep',
  'search',
  'Search',
  'glob',
  'Glob',
]);

/** Agent 工具名称集合 */
export const AGENT_TOOL_NAMES = new Set([
  'agent',
  'Agent',
  'spawnagent',
  'spawn_agent',
  'task',
  'Task',
]);

/** 工具类型 */
export type ToolType = 'bash' | 'read' | 'edit' | 'search' | 'agent' | 'generic';

/**
 * 规范化工具名称（小写 + 移除下划线和连字符）
 * @param name 原始工具名称
 * @returns 规范化后的名称
 */
export function normalizeToolName(name?: string): string {
  if (!name) return '';
  return name.trim().toLowerCase().replace(/[-_]/g, '');
}

/**
 * 判断工具名称是否属于指定工具集合
 * @param name 工具名称
 * @param toolNames 工具名称集合
 * @returns 是否匹配
 */
export function isToolName(name: string | undefined, toolNames: Set<string>): boolean {
  if (!name) return false;
  const normalized = normalizeToolName(name);
  return toolNames.has(normalized);
}

/**
 * 获取工具类型
 * @param name 工具名称
 * @returns 工具类型
 */
export function getToolType(name: string): ToolType {
  const normalized = normalizeToolName(name);

  if (READ_TOOL_NAMES.has(normalized)) return 'read';
  if (EDIT_TOOL_NAMES.has(normalized)) return 'edit';
  if (BASH_TOOL_NAMES.has(normalized)) return 'bash';
  if (SEARCH_TOOL_NAMES.has(normalized)) return 'search';
  if (AGENT_TOOL_NAMES.has(normalized)) return 'agent';

  return 'generic';
}

/** 文件路径目标信息 */
export interface ToolTargetInfo {
  /** 原始路径（input 中的值） */
  rawPath: string;
  /** 文件名（不含路径） */
  cleanFileName: string;
  /** 显示路径（相对路径优先） */
  displayPath: string;
  /** 打开路径（绝对路径） */
  openPath: string;
  /** 是否是文件 */
  isFile: boolean;
  /** 是否是目录 */
  isDirectory: boolean;
  /** 可选起始行 */
  lineStart?: number;
  /** 可选结束行 */
  lineEnd?: number;
}

/** 行号信息 */
export interface LineInfo {
  start?: number;
  end?: number;
}
