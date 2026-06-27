import {type ReactNode, useEffect, useRef, useState} from 'react';
import {AlertTriangle, Bot, CheckCircle2, FilePenLine, ListChecks, Loader2, Server} from 'lucide-react';
import {useTranslation} from 'react-i18next';
import {
    type ChatStatusEditSummary,
    type ChatStatusSummary,
    type ChatStatusToolSummary,
    getChatStatusEditKey,
} from '../../utils/chatStatusSummary';
import type {ChatMcpAvailabilityServerSummary, ChatMcpAvailabilitySummary} from '../../utils/chatMcpStatus';
import {cn} from '../../utils/cn';

type ChatInputStatusTab = 'tasks' | 'subagents' | 'edits' | 'mcp';

interface ChatInputStatusTabsProps {
    statusSummary: ChatStatusSummary;
    isStreaming?: boolean;
    selectedEditKey?: string | null;
    onSelectedEditChange?: (edit: ChatStatusEditSummary) => void;
    onSelectTool?: (tool: ChatStatusToolSummary) => void;
    defaultOpenTab?: ChatInputStatusTab | null;
    mcpStatus?: ChatMcpAvailabilitySummary;
    collapseStatusTabsOnDesktop?: boolean;
}

const MAX_PANEL_ITEMS = 8;

function latestFirst<T>(items: T[]): T[] {
    return [...items].reverse();
}

export function shouldDismissInputStatusPopoverForPointer(
    root: Pick<Node, 'contains'> | null,
    target: Node | null,
): boolean {
    return Boolean(root && target && !root.contains(target));
}

export function shouldDismissInputStatusPopoverForKey(key: string): boolean {
    return key === 'Escape';
}

export function getInputStatusTabAfterToolSelection(
    currentTab: ChatInputStatusTab | null,
    canSelectTool: boolean,
): ChatInputStatusTab | null {
    return canSelectTool ? null : currentTab;
}

export function getInputStatusTabAfterEditSelection(
    currentTab: ChatInputStatusTab | null,
    canSelectEdit: boolean,
): ChatInputStatusTab | null {
    return canSelectEdit ? null : currentTab;
}

function getStatusIcon(status: ChatStatusToolSummary['status']) {
    if (status === 'pending') return <Loader2 size={13} className="mt-0.5 flex-shrink-0 animate-spin text-warning" />;
    if (status === 'error') return <AlertTriangle size={13} className="mt-0.5 flex-shrink-0 text-error" />;
    return <CheckCircle2 size={13} className="mt-0.5 flex-shrink-0 text-success" />;
}

function getToolJumpLabelKey(tool: ChatStatusToolSummary) {
    return tool.type === 'agent'
        ? 'chat.layout.scrollToSubagentActivity'
        : 'chat.layout.scrollToToolTask';
}

function getToolJumpFallbackLabel(tool: ChatStatusToolSummary) {
    const target = tool.summary || tool.label;
    return `${tool.type === 'agent' ? 'Jump to subagent activity' : 'Jump to tool task'}: ${target}`;
}

function formatMoreStatusItemsLabel(count: number, singularLabel: string) {
    return `+${count} more ${singularLabel}${count === 1 ? '' : 's'}`;
}

function getEditStatsDescriptionId(edit: ChatStatusEditSummary) {
    const editKey = getChatStatusEditKey(edit);
    const safeKey = editKey
        .toLowerCase()
        .replace(/[^a-z0-9_-]+/g, '-')
        .replace(/^-+|-+$/g, '');
    return `chat-input-status-edit-stats-${safeKey || 'unknown'}`;
}

type ChatInputStatusEmptyPanel = 'tasks' | 'subagents' | 'edits';

const INPUT_STATUS_EMPTY_PANEL_LABELS: Record<ChatInputStatusEmptyPanel, {key: string; fallback: string}> = {
    tasks: {
        key: 'chat.layout.inputStatusNoTasks',
        fallback: 'No task or tool activity yet',
    },
    subagents: {
        key: 'chat.layout.inputStatusNoSubagents',
        fallback: 'No subagent calls yet',
    },
    edits: {
        key: 'chat.layout.inputStatusNoEdits',
        fallback: 'No file edits yet',
    },
};

export function getInputStatusEmptyPanelLabel(
    panel: ChatInputStatusEmptyPanel,
    t: (key: string) => string,
): string {
    const label = INPUT_STATUS_EMPTY_PANEL_LABELS[panel];
    const translated = t(label.key);
    return translated === label.key ? label.fallback : translated;
}

export default function ChatInputStatusTabs({
    statusSummary,
    isStreaming = false,
    selectedEditKey,
    onSelectedEditChange,
    onSelectTool,
    defaultOpenTab = null,
    mcpStatus,
    collapseStatusTabsOnDesktop = false,
}: ChatInputStatusTabsProps) {
    const {t} = useTranslation();
    const [openTab, setOpenTab] = useState<ChatInputStatusTab | null>(defaultOpenTab);
    const popoverRootRef = useRef<HTMLDivElement>(null);
    const toolTimeline = statusSummary.toolTimeline ?? (statusSummary.activeTool ? [statusSummary.activeTool] : []);
    const taskTools = toolTimeline.filter((tool) => tool.type !== 'agent');
    const agentTools = statusSummary.agentTools ?? toolTimeline.filter((tool) => tool.type === 'agent');
    const edits = statusSummary.allEdits.length > 0 ? statusSummary.allEdits : statusSummary.recentEdits;
    const hasTasks = taskTools.length > 0;
    const hasSubagents = agentTools.length > 0;
    const hasEdits = edits.length > 0;
    const hasMcpStatus = Boolean(mcpStatus && (mcpStatus.totalServers > 0 || mcpStatus.loading || mcpStatus.error));
    const hasExpandableStatusTabs = hasTasks || hasSubagents || hasEdits || hasMcpStatus;
    const visibleTabs = new Set<ChatInputStatusTab>([
        ...(hasTasks ? ['tasks' as const] : []),
        ...(hasSubagents ? ['subagents' as const] : []),
        ...(hasEdits ? ['edits' as const] : []),
        ...(hasMcpStatus ? ['mcp' as const] : []),
    ]);
    const activeOpenTab = openTab && visibleTabs.has(openTab) ? openTab : null;
    const recentTools = latestFirst(taskTools).slice(0, MAX_PANEL_ITEMS);
    const recentAgents = latestFirst(agentTools).slice(0, MAX_PANEL_ITEMS);
    const visibleEdits = edits.slice(0, MAX_PANEL_ITEMS);
    const hiddenTaskCount = taskTools.length - MAX_PANEL_ITEMS;
    const hiddenSubagentCount = agentTools.length - MAX_PANEL_ITEMS;
    const hiddenEditCount = edits.length - MAX_PANEL_ITEMS;
    const completedTools = taskTools.filter((tool) => tool.status === 'completed').length;
    const pendingTasks = taskTools.some((tool) => tool.status === 'pending');
    const completedAgents = agentTools.filter((tool) => tool.status === 'completed').length;
    const pendingAgents = agentTools.some((tool) => tool.status === 'pending');
    const activeTabClass = 'border-primary/40 bg-primary/10 text-primary shadow-sm';
    const inactiveTabClass = 'border-transparent bg-base-100/55 text-base-content/60 hover:bg-base-100 hover:text-base-content/80';
    const translateWithFallback = (key: string, fallback: string, options?: Record<string, unknown>) => {
        const translated = options ? t(key, options) : t(key);
        return translated === key ? fallback : translated;
    };
    const taskTabLabel = translateWithFallback('chat.layout.inputStatusTasks', 'Tasks');
    const taskTabStat = translateWithFallback(
        'chat.layout.inputStatusProgress',
        `${completedTools}/${taskTools.length}`,
        {completed: completedTools, total: taskTools.length},
    );
    const subagentTabLabel = translateWithFallback('chat.layout.inputStatusSubagents', 'Subagents');
    const subagentTabStat = translateWithFallback(
        'chat.layout.inputStatusProgress',
        `${completedAgents}/${agentTools.length}`,
        {completed: completedAgents, total: agentTools.length},
    );
    const editTabLabel = translateWithFallback('chat.layout.inputStatusEdits', 'Edits');
    const editTabStat = translateWithFallback(
        'chat.layout.inputStatusEditStats',
        `+${statusSummary.totalAdditions} / -${statusSummary.totalDeletions}`,
        {additions: statusSummary.totalAdditions, deletions: statusSummary.totalDeletions},
    );
    const mcpTabLabel = translateWithFallback('chat.layout.mcpStatus', 'MCP');
    const mcpUnknownTransportLabel = translateWithFallback('chat.layout.mcpLiveUnknown', 'Unknown');
    const mcpEnabledLabel = translateWithFallback('chat.layout.mcpEnabled', 'Enabled');
    const mcpDisabledLabel = translateWithFallback('chat.layout.mcpDisabled', 'Disabled');
    const mcpConfigurationErrorLabel = translateWithFallback('chat.layout.mcpConfigurationError', 'Configuration error');
    const mcpLoadingLabel = translateWithFallback('chat.layout.mcpLoading', 'Loading MCP configuration...');
    const mcpConfiguredServersLabel = translateWithFallback('chat.layout.mcpConfiguredServers', 'Configured servers');
    const mcpNoServersLabel = translateWithFallback('chat.layout.mcpNoServers', 'No MCP servers configured');
    const statusDetailsRegionLabel = translateWithFallback('chat.layout.inputStatusDetailsRegion', 'Status details');
    const getMcpServerStatusLabel = (server: ChatMcpAvailabilityServerSummary) => (
        server.enabled ? mcpEnabledLabel : mcpDisabledLabel
    );
    const getMcpServerStatusTargetLabel = (server: ChatMcpAvailabilityServerSummary) => (
        `${server.name}: ${getMcpServerStatusLabel(server)}`
    );
    const getMcpServerNameTargetLabel = (server: ChatMcpAvailabilityServerSummary) => translateWithFallback(
        'chat.layout.mcpServerName',
        `MCP server: ${server.name}`,
        {server: server.name},
    );
    const getMcpServerTransportTargetLabel = (server: ChatMcpAvailabilityServerSummary) => {
        const transport = server.transport ?? mcpUnknownTransportLabel;
        return translateWithFallback(
            'chat.layout.mcpServerTransport',
            `MCP server transport: ${server.name} · ${transport}`,
            {server: server.name, transport},
        );
    };
    const getMcpConfigurationErrorTargetLabel = (error: string) => (
        `${mcpTabLabel}: ${mcpConfigurationErrorLabel} · ${error}`
    );
    const mcpLoadingTargetLabel = `${mcpTabLabel}: ${mcpLoadingLabel}`;
    const mcpConfiguredSummaryLabel = mcpStatus
        ? translateWithFallback('chat.layout.mcpEnabledSummary', `${mcpStatus.enabledServers} / ${mcpStatus.totalServers} available`, {
            enabled: mcpStatus.enabledServers,
            total: mcpStatus.totalServers,
        })
        : '';
    const mcpTabStat = mcpStatus ? `${mcpStatus.enabledServers} / ${mcpStatus.totalServers}` : '';
    const mcpTabAccessibleLabel = mcpStatus?.error
        ? getMcpConfigurationErrorTargetLabel(mcpStatus.error)
        : mcpStatus?.loading
            ? mcpLoadingTargetLabel
            : `${mcpTabLabel}: ${mcpConfiguredSummaryLabel}`;
    const emptyTasksLabel = getInputStatusEmptyPanelLabel('tasks', t);
    const emptySubagentsLabel = getInputStatusEmptyPanelLabel('subagents', t);
    const emptyEditsLabel = getInputStatusEmptyPanelLabel('edits', t);

    const toggleTab = (tab: ChatInputStatusTab) => {
        setOpenTab((current) => (current === tab ? null : tab));
    };

    useEffect(() => {
        if (!activeOpenTab) return;

        const handlePointerDown = (event: MouseEvent) => {
            const target = event.target instanceof Node ? event.target : null;
            if (shouldDismissInputStatusPopoverForPointer(popoverRootRef.current, target)) {
                setOpenTab(null);
            }
        };
        const handleKeyDown = (event: KeyboardEvent) => {
            if (shouldDismissInputStatusPopoverForKey(event.key)) {
                setOpenTab(null);
            }
        };

        document.addEventListener('mousedown', handlePointerDown);
        document.addEventListener('keydown', handleKeyDown);
        return () => {
            document.removeEventListener('mousedown', handlePointerDown);
            document.removeEventListener('keydown', handleKeyDown);
        };
    }, [activeOpenTab]);

    if (!hasTasks && !hasSubagents && !hasEdits && !hasMcpStatus) {
        return null;
    }

    const statusLabel = (status: ChatStatusToolSummary['status']) => {
        if (status === 'pending') return translateWithFallback('tools.pending', 'Pending');
        if (status === 'error') return translateWithFallback('tools.failed', 'Failed');
        return translateWithFallback('common.success', 'Success');
    };
    const getToolStatusTargetLabel = (tool: ChatStatusToolSummary) => {
        const target = tool.summary || tool.detail || tool.label;
        return `${tool.label}: ${target} · ${statusLabel(tool.status)}`;
    };
    const getEditStatsTargetLabel = (edit: ChatStatusEditSummary) => translateWithFallback(
        'chat.layout.inputStatusEditFileStats',
        `Edit stats: ${edit.displayPath} · +${edit.additions} / -${edit.deletions}`,
        {file: edit.displayPath, additions: edit.additions, deletions: edit.deletions},
    );
    const moreTaskToolsLabel = hiddenTaskCount > 0
        ? translateWithFallback(
            'chat.layout.inputStatusMoreTools',
            formatMoreStatusItemsLabel(hiddenTaskCount, 'tool task'),
            {count: hiddenTaskCount},
        )
        : '';
    const moreSubagentsLabel = hiddenSubagentCount > 0
        ? translateWithFallback(
            'chat.layout.inputStatusMoreSubagents',
            formatMoreStatusItemsLabel(hiddenSubagentCount, 'subagent'),
            {count: hiddenSubagentCount},
        )
        : '';
    const moreEditsLabel = hiddenEditCount > 0
        ? translateWithFallback(
            'chat.layout.inputStatusMoreEdits',
            formatMoreStatusItemsLabel(hiddenEditCount, 'edit'),
            {count: hiddenEditCount},
        )
        : '';

    const handleSelectToolRow = (tool: ChatStatusToolSummary) => {
        setOpenTab(getInputStatusTabAfterToolSelection(activeOpenTab, Boolean(onSelectTool)));
        onSelectTool?.(tool);
    };

    const handleSelectEditRow = (edit: ChatStatusEditSummary) => {
        setOpenTab(getInputStatusTabAfterEditSelection(activeOpenTab, Boolean(onSelectedEditChange)));
        onSelectedEditChange?.(edit);
    };

    const renderToolRow = (tool: ChatStatusToolSummary) => {
        const toolJumpLabelKey = getToolJumpLabelKey(tool);
        const translatedToolJumpLabel = t(toolJumpLabelKey, {tool: tool.summary || tool.label});
        const toolJumpLabel = translatedToolJumpLabel === toolJumpLabelKey
            ? getToolJumpFallbackLabel(tool)
            : translatedToolJumpLabel;
        const toolStatusTargetLabel = getToolStatusTargetLabel(tool);

        return (
            <button
                key={tool.toolId}
                type="button"
                className={cn(
                    'flex w-full min-w-0 items-start gap-2 rounded-md bg-base-200/45 px-2 py-1.5 text-left transition-colors',
                    'hover:bg-base-200/80 focus:outline-none focus:ring-2 focus:ring-primary/30',
                    'disabled:cursor-default disabled:opacity-100 disabled:hover:bg-base-200/45',
                )}
                title={toolJumpLabel}
                aria-label={toolJumpLabel}
                data-target-tool-id={tool.toolId}
                disabled={!onSelectTool}
                onClick={() => handleSelectToolRow(tool)}
            >
                {getStatusIcon(tool.status)}
                <div className="min-w-0 flex-1">
                    <div className="flex min-w-0 items-center gap-1.5">
                        <span className={`tool-command-chip ${tool.accentClass}`}>{tool.label}</span>
                        <span
                            className={`tool-state-pill ${tool.status}`}
                            title={toolStatusTargetLabel}
                            aria-label={toolStatusTargetLabel}
                        >
                            {statusLabel(tool.status)}
                        </span>
                    </div>
                    <div className="mt-1 truncate text-[11px] font-medium text-base-content/75">
                        {tool.summary}
                    </div>
                    {tool.detail && (
                        <div className="mt-0.5 truncate text-[10px] text-base-content/45">
                            {tool.detail}
                        </div>
                    )}
                </div>
            </button>
        );
    };

    const renderTasksPanel = () => (
        <div className="space-y-1.5">
            {recentTools.length > 0 ? (
                <>
                    {recentTools.map(renderToolRow)}
                    {hiddenTaskCount > 0 && (
                        <div className="px-1 text-[10px] text-base-content/40">
                            {moreTaskToolsLabel}
                        </div>
                    )}
                </>
            ) : (
                <div className="rounded-md bg-base-200/35 px-2 py-2 text-[11px] text-base-content/45">
                    {emptyTasksLabel}
                </div>
            )}
        </div>
    );

    const renderSubagentsPanel = () => (
        <div className="space-y-1.5">
            {recentAgents.length > 0 ? (
                <>
                    {recentAgents.map(renderToolRow)}
                    {hiddenSubagentCount > 0 && (
                        <div className="px-1 text-[10px] text-base-content/40">
                            {moreSubagentsLabel}
                        </div>
                    )}
                </>
            ) : (
                <div className="rounded-md bg-base-200/35 px-2 py-2 text-[11px] text-base-content/45">
                    {emptySubagentsLabel}
                </div>
            )}
        </div>
    );

    const renderEditsPanel = () => (
        <div className="space-y-1.5">
            {visibleEdits.length > 0 ? (
                <>
                    {visibleEdits.map((edit) => {
                        const editKey = getChatStatusEditKey(edit);
                        const selected = selectedEditKey === editKey;
                        const inspectDiffLabelKey = selected
                            ? 'chat.layout.inspectCurrentFullDiff'
                            : 'chat.layout.inspectFullDiff';
                        const translatedInspectDiffLabel = t(inspectDiffLabelKey, {file: edit.displayPath});
                        const inspectDiffLabel = translatedInspectDiffLabel === inspectDiffLabelKey
                            ? `${selected ? 'Current full diff' : 'Inspect full diff'}: ${edit.displayPath}`
                            : translatedInspectDiffLabel;
                        const editStatsTargetLabel = getEditStatsTargetLabel(edit);
                        const editStatsDescriptionId = getEditStatsDescriptionId(edit);
                        return (
                            <button
                                key={editKey}
                                type="button"
                                className={cn(
                                    'flex w-full min-w-0 items-center gap-2 rounded-md bg-base-200/45 px-2 py-1.5 text-left transition-colors',
                                    'hover:bg-base-200/80 focus:outline-none focus:ring-2 focus:ring-primary/30',
                                    'disabled:cursor-default disabled:opacity-100 disabled:hover:bg-base-200/45',
                                    selected && 'chat-input-status-edit-selected ring-1 ring-primary/30 bg-primary/10',
                                )}
                                title={inspectDiffLabel}
                                aria-label={inspectDiffLabel}
                                aria-describedby={editStatsDescriptionId}
                                aria-current={selected ? 'true' : undefined}
                                disabled={!onSelectedEditChange}
                                onClick={() => handleSelectEditRow(edit)}
                            >
                                <FilePenLine size={13} className="flex-shrink-0 text-base-content/45" />
                                <div className="min-w-0 flex-1">
                                    <div className="truncate text-[11px] font-medium text-base-content/75">
                                        {edit.displayPath}
                                    </div>
                                    {(edit.lineStart || edit.lineEnd) && (
                                        <div className="truncate text-[10px] text-base-content/40">
                                            {edit.lineStart ? `L${edit.lineStart}${edit.lineEnd && edit.lineEnd !== edit.lineStart ? `-L${edit.lineEnd}` : ''}` : ''}
                                        </div>
                                    )}
                                </div>
                                <div
                                    id={editStatsDescriptionId}
                                    className="flex flex-shrink-0 items-center gap-1 text-[11px] font-medium"
                                    title={editStatsTargetLabel}
                                    aria-label={editStatsTargetLabel}
                                >
                                    <span className="text-success">+{edit.additions}</span>
                                    <span className="text-error">-{edit.deletions}</span>
                                </div>
                            </button>
                        );
                    })}
                    {hiddenEditCount > 0 && (
                        <div className="px-1 text-[10px] text-base-content/40">
                            {moreEditsLabel}
                        </div>
                    )}
                </>
            ) : (
                <div className="rounded-md bg-base-200/35 px-2 py-2 text-[11px] text-base-content/45">
                    {emptyEditsLabel}
                </div>
            )}
        </div>
    );

    const renderMcpServerRow = (server: ChatMcpAvailabilityServerSummary) => {
        const mcpServerStatusLabel = getMcpServerStatusLabel(server);
        const mcpServerNameTargetLabel = getMcpServerNameTargetLabel(server);
        const mcpServerStatusTargetLabel = getMcpServerStatusTargetLabel(server);
        const mcpServerTransportTargetLabel = getMcpServerTransportTargetLabel(server);

        return (
            <div
                key={server.id}
                className="chat-input-status-mcp-server flex min-w-0 items-center gap-2 rounded-md bg-base-200/45 px-2 py-1.5"
            >
                <Server size={13} className="flex-shrink-0 text-base-content/45" />
                <div className="min-w-0 flex-1">
                    <div
                        className="truncate text-[11px] font-medium text-base-content/75"
                        title={mcpServerNameTargetLabel}
                        aria-label={mcpServerNameTargetLabel}
                    >
                        {server.name}
                    </div>
                    <div
                        className="truncate text-[10px] text-base-content/40"
                        title={mcpServerTransportTargetLabel}
                        aria-label={mcpServerTransportTargetLabel}
                    >
                        {server.transport ?? mcpUnknownTransportLabel}
                    </div>
                </div>
                <span
                    className={`tool-state-pill ${server.enabled ? 'completed' : 'pending'}`}
                    title={mcpServerStatusTargetLabel}
                    aria-label={mcpServerStatusTargetLabel}
                >
                    {mcpServerStatusLabel}
                </span>
            </div>
        );
    };

    const renderMcpPanel = () => {
        if (!mcpStatus) return null;
        const mcpConfigurationErrorTargetLabel = mcpStatus.error
            ? getMcpConfigurationErrorTargetLabel(mcpStatus.error)
            : '';
        const hiddenMcpServerCount = mcpStatus.servers.length - MAX_PANEL_ITEMS;
        const moreMcpServersLabel = hiddenMcpServerCount > 0
            ? translateWithFallback(
                'chat.layout.inputStatusMoreMcpServers',
                `+${hiddenMcpServerCount} more MCP server${hiddenMcpServerCount === 1 ? '' : 's'}`,
                {count: hiddenMcpServerCount},
            )
            : '';

        return (
            <div className="space-y-1.5">
                {mcpStatus.error && (
                    <div
                        className="rounded-md bg-error/10 px-2 py-1.5 text-[11px] text-error/85"
                        title={mcpConfigurationErrorTargetLabel}
                        aria-label={mcpConfigurationErrorTargetLabel}
                    >
                        {mcpStatus.error}
                    </div>
                )}
                {mcpStatus.loading && (
                    <div
                        className="flex items-center gap-1.5 rounded-md bg-base-200/35 px-2 py-2 text-[11px] text-base-content/45"
                        title={mcpLoadingTargetLabel}
                        aria-label={mcpLoadingTargetLabel}
                    >
                        <Loader2 size={12} className="animate-spin text-warning" />
                        {mcpLoadingLabel}
                    </div>
                )}
                {mcpStatus.servers.length > 0 ? (
                    <>
                        <div className="flex items-center justify-between px-1 text-[10px] text-base-content/40">
                            <span>{mcpConfiguredServersLabel}</span>
                            <span>{mcpStatus.enabledServers} / {mcpStatus.totalServers}</span>
                        </div>
                        {mcpStatus.servers.slice(0, MAX_PANEL_ITEMS).map(renderMcpServerRow)}
                        {hiddenMcpServerCount > 0 && (
                            <div className="px-1 text-[10px] text-base-content/40">
                                {moreMcpServersLabel}
                            </div>
                        )}
                    </>
                ) : (
                    !mcpStatus.loading && (
                        <div className="rounded-md bg-base-200/35 px-2 py-2 text-[11px] text-base-content/45">
                            {mcpNoServersLabel}
                        </div>
                    )
                )}
            </div>
        );
    };

    const renderPanel = () => {
        if (activeOpenTab === 'tasks') return renderTasksPanel();
        if (activeOpenTab === 'subagents') return renderSubagentsPanel();
        if (activeOpenTab === 'edits') return renderEditsPanel();
        if (activeOpenTab === 'mcp') return renderMcpPanel();
        return null;
    };

    const renderTabButton = (
        tab: ChatInputStatusTab,
        className: string,
        icon: ReactNode,
        label: string,
        stat: string,
        showSpinner = false,
        accessibleLabelOverride?: string,
    ) => {
        const accessibleLabel = accessibleLabelOverride ?? `${label} ${stat}`;

        return (
            <button
                type="button"
                className={cn(
                    'chat-input-status-tab flex min-w-0 items-center justify-center gap-1.5 rounded-md border px-1.5 py-1.5 text-[11px] font-medium transition-colors sm:px-2',
                    activeOpenTab === tab ? activeTabClass : inactiveTabClass,
                    className,
                    collapseStatusTabsOnDesktop && 'xl:hidden',
                )}
                aria-expanded={activeOpenTab === tab}
                aria-label={accessibleLabel}
                title={accessibleLabel}
                onClick={() => toggleTab(tab)}
            >
                {icon}
                <span className="chat-input-status-tab-label hidden sm:inline max-w-[5rem] truncate">{label}</span>
                <span className="chat-input-status-count-pill flex-shrink-0 rounded-full bg-base-200/80 px-1.5 py-0.5 text-[10px] leading-none text-base-content/55">
                    {stat}
                </span>
                {showSpinner && <Loader2 size={11} className="flex-shrink-0 animate-spin text-warning" />}
            </button>
        );
    };

    return (
        <div
            className={cn(
                'chat-input-status-tabs bg-base-200/20 px-2 pt-2 sm:px-3',
                collapseStatusTabsOnDesktop && hasExpandableStatusTabs && 'xl:hidden',
            )}
        >
            <div className="relative w-full" ref={popoverRootRef}>
                <div className="flex flex-wrap items-stretch gap-1 rounded-md border border-base-300 bg-base-100/70 p-1 shadow-sm shadow-base-300/20">
                    {hasTasks && renderTabButton(
                        'tasks',
                        'chat-input-status-tab-tasks',
                        <ListChecks size={13} className="flex-shrink-0" />,
                        taskTabLabel,
                        taskTabStat,
                        isStreaming && pendingTasks,
                    )}
                    {hasSubagents && renderTabButton(
                        'subagents',
                        'chat-input-status-tab-subagents',
                        <Bot size={13} className="flex-shrink-0" />,
                        subagentTabLabel,
                        subagentTabStat,
                        isStreaming && pendingAgents,
                    )}
                    {hasEdits && renderTabButton(
                        'edits',
                        'chat-input-status-tab-edits',
                        <FilePenLine size={13} className="flex-shrink-0" />,
                        editTabLabel,
                        editTabStat,
                    )}
                    {hasMcpStatus && mcpStatus && renderTabButton(
                        'mcp',
                        'chat-input-status-tab-mcp',
                        <Server size={13} className="flex-shrink-0" />,
                        mcpTabLabel,
                        mcpTabStat,
                        mcpStatus.loading,
                        mcpTabAccessibleLabel,
                    )}
                </div>
                {activeOpenTab && (
                    <div
                        className={cn(
                            'chat-input-status-panel chat-input-status-popover-panel absolute bottom-full left-0 right-0 z-[30] mb-1 max-h-[min(20rem,45vh)] overflow-y-auto rounded-md border border-base-300 bg-base-100/95 p-2 text-xs shadow-lg shadow-base-300/20 focus:outline-none focus:ring-2 focus:ring-primary/25',
                            collapseStatusTabsOnDesktop && 'xl:hidden',
                        )}
                        role="region"
                        tabIndex={0}
                        aria-label={statusDetailsRegionLabel}
                    >
                        {renderPanel()}
                    </div>
                )}
            </div>
        </div>
    );
}
