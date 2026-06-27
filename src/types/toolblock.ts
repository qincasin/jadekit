// 工具块组件相关类型定义

import type { ToolResultBlock } from './chat';

/** GenericToolBlock 组件的 Props */
export interface GenericToolBlockProps {
    name: string;
    input: Record<string, unknown>;
    result?: ToolResultBlock | null;
    toolId: string;
}

/** 工具状态 */
export type ToolStatus = 'pending' | 'completed' | 'error';
