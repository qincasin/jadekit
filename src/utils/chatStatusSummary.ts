import type {ChatMessage, ToolResultBlock, ToolUseBlock} from '../types/chat';
import {getContentBlocksFromRaw} from './chatMessageFlow';
import {
    collectEditToolItems,
    type DiffPreviewLine,
    extractAgentToolMeta,
    extractResultText,
    formatLineRange,
    getToolDisplayStatus,
    mergeEditToolItemsByFile,
    resolveToolTarget,
    summarizeAgentToolHeader,
    summarizeCommand,
    summarizeGenericTool,
    summarizeGroupBashItemResult,
    summarizeSearchInput,
    summarizeSearchResultText,
    summarizeToolResultText,
    type ToolDisplayStatus,
} from './toolPresentation';
import {getToolType} from '../types/tools';

const RECENT_EDIT_LIMIT = 4;

export interface ChatStatusToolSummary {
    toolId: string;
    type: 'bash' | 'read' | 'edit' | 'search' | 'agent' | 'generic';
    label: string;
    accentClass: string;
    summary: string;
    detail?: string;
    status: ToolDisplayStatus;
}

export interface ChatStatusEditSummary {
    toolId: string;
    displayPath: string;
    openPath: string;
    lineStart?: number;
    lineEnd?: number;
    additions: number;
    deletions: number;
    diffPreviewLines: DiffPreviewLine[];
    status: ToolDisplayStatus;
}

export interface ChatStatusSummary {
    activeTool?: ChatStatusToolSummary;
    toolTimeline?: ChatStatusToolSummary[];
    agentTools?: ChatStatusToolSummary[];
    recentEdits: ChatStatusEditSummary[];
    allEdits: ChatStatusEditSummary[];
    touchedFileCount: number;
    totalAdditions: number;
    totalDeletions: number;
    pendingToolCount: number;
    completedToolCount: number;
    errorToolCount: number;
}

export function getChatStatusEditKey(edit: ChatStatusEditSummary): string {
    return `${edit.openPath || edit.displayPath}:${edit.toolId}`;
}

function hasCompleteInputStatusSignal(summary: ChatStatusSummary): boolean {
    return Boolean(
        (summary.toolTimeline?.length ?? 0) > 0
        || (summary.agentTools?.length ?? 0) > 0
        || summary.allEdits.length > 0,
    );
}

export function mergeChatInputStatusSummary(
    visibleSummary: ChatStatusSummary,
    completeSummary?: ChatStatusSummary | null,
): ChatStatusSummary {
    if (!completeSummary || !hasCompleteInputStatusSignal(completeSummary)) {
        return visibleSummary;
    }

    return {
        activeTool: visibleSummary.activeTool?.status === 'pending'
            ? visibleSummary.activeTool
            : completeSummary.activeTool ?? visibleSummary.activeTool,
        toolTimeline: completeSummary.toolTimeline ?? visibleSummary.toolTimeline,
        agentTools: completeSummary.agentTools ?? visibleSummary.agentTools,
        recentEdits: completeSummary.allEdits.length > 0
            ? completeSummary.recentEdits
            : visibleSummary.recentEdits,
        allEdits: completeSummary.allEdits.length > 0
            ? completeSummary.allEdits
            : visibleSummary.allEdits,
        touchedFileCount: completeSummary.allEdits.length > 0
            ? completeSummary.touchedFileCount
            : visibleSummary.touchedFileCount,
        totalAdditions: completeSummary.allEdits.length > 0
            ? completeSummary.totalAdditions
            : visibleSummary.totalAdditions,
        totalDeletions: completeSummary.allEdits.length > 0
            ? completeSummary.totalDeletions
            : visibleSummary.totalDeletions,
        pendingToolCount: completeSummary.pendingToolCount,
        completedToolCount: completeSummary.completedToolCount,
        errorToolCount: completeSummary.errorToolCount,
    };
}

interface ToolTimelineEntry {
    block: ToolUseBlock;
    result: ToolResultBlock | null;
    status: ToolDisplayStatus;
}

function collectToolTimeline(messages: ChatMessage[]): ToolTimelineEntry[] {
    const resultMap = new Map<string, ToolResultBlock>();

    messages.forEach((message) => {
        getContentBlocksFromRaw(message.raw).forEach((block) => {
            if (block.type === 'tool_result') {
                resultMap.set(block.tool_use_id, block);
            }
        });
    });

    const entries: ToolTimelineEntry[] = [];

    messages.forEach((message) => {
        getContentBlocksFromRaw(message.raw).forEach((block) => {
            if (block.type !== 'tool_use') return;
            const toolResult = resultMap.get(block.id) ?? null;
            entries.push({
                block,
                result: toolResult,
                status: getToolDisplayStatus(toolResult),
            });
        });
    });

    return entries;
}

function summarizeToolEntry(entry: ToolTimelineEntry): ChatStatusToolSummary {
    const toolType = getToolType(entry.block.name);
    const resultText = entry.result ? summarizeToolResultText(extractResultText(entry.result)) : '';

    if (toolType === 'bash') {
        const command = typeof entry.block.input.command === 'string' ? entry.block.input.command : '';
        const commandSummary = summarizeCommand(command);
        return {
            toolId: entry.block.id,
            type: 'bash',
            label: commandSummary.label,
            accentClass: commandSummary.accentClass,
            summary: commandSummary.summary,
            detail: entry.result ? summarizeGroupBashItemResult(entry.result) : '',
            status: entry.status,
        };
    }

    if (toolType === 'read') {
        const target = resolveToolTarget(entry.block.input);
        const lineStart = target?.lineStart ?? (
            typeof entry.block.input.start_line === 'number' ? entry.block.input.start_line : undefined
        );
        const lineEnd = target?.lineEnd ?? (
            typeof entry.block.input.end_line === 'number' ? entry.block.input.end_line : undefined
        );
        return {
            toolId: entry.block.id,
            type: 'read',
            label: 'Read',
            accentClass: 'tool-command-read',
            summary: target?.displayPath ?? entry.block.name,
            detail: lineStart ? formatLineRange({ start: lineStart, end: lineEnd }) : resultText,
            status: entry.status,
        };
    }

    if (toolType === 'edit') {
        const editItems = collectEditToolItems([entry.block], (toolId) => (
            toolId === entry.block.id ? entry.result : null
        ));
        const firstItem = editItems[0];
        return {
            toolId: entry.block.id,
            type: 'edit',
            label: 'Edit',
            accentClass: 'tool-command-patch',
            summary: firstItem
                ? (editItems.length > 1 ? `${editItems.length} files` : firstItem.displayPath)
                : entry.block.name,
            detail: firstItem
                ? `+${editItems.reduce((sum, item) => sum + item.additions, 0)} / -${editItems.reduce((sum, item) => sum + item.deletions, 0)}`
                : resultText,
            status: entry.status,
        };
    }

    if (toolType === 'search') {
        const query = summarizeSearchInput(entry.block.input);
        const searchSummary = entry.result ? summarizeSearchResultText(extractResultText(entry.result)) : null;
        const secondary = searchSummary && searchSummary.fileCount > 0
            ? `${searchSummary.matchCount} matches · ${searchSummary.fileCount} files`
            : resultText;

        return {
            toolId: entry.block.id,
            type: 'search',
            label: 'Search',
            accentClass: 'tool-command-search',
            summary: query || entry.block.name,
            detail: secondary,
            status: entry.status,
        };
    }

    if (toolType === 'agent') {
        const meta = extractAgentToolMeta(entry.block.input, entry.result);
        const toolKind = entry.block.name.toLowerCase().includes('task') || entry.block.name.toLowerCase().includes('spawn')
            ? 'task'
            : 'agent';
        const summary = summarizeAgentToolHeader(
            meta,
            entry.result,
            toolKind,
        );
        const visibleIdentity = [meta.subagentType, meta.nickname].filter(Boolean).join(' · ');
        const visibleDetail = [
            visibleIdentity || summary.secondarySummary,
            summary.runtimeSummary,
        ].filter(Boolean).join(' · ');

        return {
            toolId: entry.block.id,
            type: 'agent',
            label: toolKind === 'task' ? 'Task' : 'Agent',
            accentClass: 'tool-command-plan',
            summary: summary.primarySummary || entry.block.name,
            detail: visibleDetail,
            status: entry.status,
        };
    }

    const genericSummary = summarizeGenericTool(entry.block.name, entry.block.input);
    return {
        toolId: entry.block.id,
        type: 'generic',
        label: genericSummary.label,
        accentClass: genericSummary.accentClass,
        summary: genericSummary.summary,
        detail: resultText,
        status: entry.status,
    };
}

function collectEditSummaries(entries: ToolTimelineEntry[]): ChatStatusEditSummary[] {
    const editBlocks = entries
        .filter((entry) => getToolType(entry.block.name) === 'edit')
        .map((entry) => entry.block);
    const resultById = new Map(entries.map((entry) => [entry.block.id, entry.result]));
    const editItems = collectEditToolItems(editBlocks, (toolId) => resultById.get(toolId) ?? null);

    return mergeEditToolItemsByFile(editItems, {
        order: 'last',
    }).map((item) => ({
        toolId: item.toolId,
        displayPath: item.displayPath,
        openPath: item.openPath,
        additions: item.additions,
        deletions: item.deletions,
        diffPreviewLines: item.diffPreviewLines,
        lineStart: item.lineStart,
        lineEnd: item.lineEnd,
        status: item.isError ? 'error' : item.isCompleted ? 'completed' : 'pending',
    }));
}

export function buildChatStatusSummary(messages: ChatMessage[]): ChatStatusSummary {
    const timeline = collectToolTimeline(messages);
    const toolTimeline = timeline.map(summarizeToolEntry);
    const editSummaries = collectEditSummaries(timeline);
    const recentEdits = editSummaries.slice(0, RECENT_EDIT_LIMIT);
    const pendingTool = [...timeline].reverse().find((entry) => entry.status === 'pending');
    const latestTool = timeline.length > 0 ? timeline[timeline.length - 1] : undefined;
    const activeEntry = pendingTool ?? latestTool;

    return {
        activeTool: activeEntry ? summarizeToolEntry(activeEntry) : undefined,
        toolTimeline,
        agentTools: toolTimeline.filter((tool) => tool.type === 'agent'),
        recentEdits,
        allEdits: editSummaries,
        touchedFileCount: editSummaries.length,
        totalAdditions: editSummaries.reduce((sum, item) => sum + item.additions, 0),
        totalDeletions: editSummaries.reduce((sum, item) => sum + item.deletions, 0),
        pendingToolCount: timeline.filter((entry) => entry.status === 'pending').length,
        completedToolCount: timeline.filter((entry) => entry.status === 'completed').length,
        errorToolCount: timeline.filter((entry) => entry.status === 'error').length,
    };
}
