// 工具权限拒绝检测 Hook

import { useChatStore } from '../stores/useChatStore';

/**
 * 检查工具是否被用户拒绝
 * @param toolId 工具调用 ID
 * @returns 是否被拒绝
 */
export function useIsToolDenied(toolId?: string): boolean {
  const deniedToolIds = useChatStore((state) => state.deniedToolIds);
  return toolId ? deniedToolIds.has(toolId) : false;
}
