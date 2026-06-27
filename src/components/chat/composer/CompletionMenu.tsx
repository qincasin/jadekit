import {useEffect, useRef} from 'react';
import type {LucideIcon} from 'lucide-react';
import {Bot, File, Folder, Loader2, Sparkles, Terminal} from 'lucide-react';

/** 补全项类型，决定左侧图标与展示语义。 */
export type CompletionItemKind = 'file' | 'directory' | 'command' | 'agent' | 'prompt';

export interface CompletionItem {
    id: string;
    /** 主文本：文件名 / 命令名 / 代理名 / 预设名 */
    label: string;
    /** 副文本：文件所在路径 / 命令说明 / 预设摘要 */
    description?: string;
    /** 插入到输入框的值（不含触发符），默认用 label */
    insertText?: string;
    /** 补全项类型，缺省按文件处理 */
    kind?: CompletionItemKind;
}

const KIND_ICON: Record<CompletionItemKind, LucideIcon> = {
    file: File,
    directory: Folder,
    command: Terminal,
    agent: Bot,
    prompt: Sparkles,
};

interface CompletionMenuProps {
    items: CompletionItem[];
    activeIndex: number;
    loading?: boolean;
    emptyText: string;
    loadingText: string;
    menuLabel: string;
    onSelect: (index: number) => void;
    onHover: (index: number) => void;
}

/**
 * 输入框上方的补全菜单（@文件 / #子代理 / !预设 / /命令 共用）。
 * 定位由父容器 relative 决定，菜单贴 textarea 顶部向上弹出。
 */
export function CompletionMenu({
    items,
    activeIndex,
    loading,
    emptyText,
    loadingText,
    menuLabel,
    onSelect,
    onHover,
}: CompletionMenuProps) {
    const listRef = useRef<HTMLDivElement>(null);

    // 键盘移动时把高亮项滚进可视区
    useEffect(() => {
        const el = listRef.current?.children[activeIndex] as HTMLElement | undefined;
        el?.scrollIntoView({ block: 'nearest' });
    }, [activeIndex]);

    return (
        <div
            className="absolute bottom-full left-0 mb-2 z-[10000] w-[26rem] max-w-[90vw] max-h-72 overflow-y-auto rounded-lg border border-base-300 bg-base-100 shadow-xl"
            role="listbox"
            aria-label={menuLabel}
        >
            {loading ? (
                <div
                    className="flex items-center gap-2 px-3 py-3 text-xs text-base-content/50"
                    role="status"
                    aria-live="polite"
                >
                    <Loader2 size={14} className="animate-spin" />
                    {loadingText}
                </div>
            ) : items.length === 0 ? (
                <div className="px-3 py-3 text-xs text-base-content/40">{emptyText}</div>
            ) : (
                <div ref={listRef}>
                    {items.map((item, i) => {
                        const Icon = KIND_ICON[item.kind ?? 'file'];
                        const active = i === activeIndex;
                        return (
                            <button
                                key={item.id}
                                type="button"
                                role="option"
                                aria-selected={active}
                                aria-label={item.description ? `${item.label}. ${item.description}` : item.label}
                                onMouseDown={(e) => {
                                    e.preventDefault();
                                    onSelect(i);
                                }}
                                onMouseEnter={() => onHover(i)}
                                className={`flex w-full items-center gap-2 px-3 py-1.5 text-left
                                    ${active ? 'bg-primary/10' : 'hover:bg-base-200'}`}
                            >
                                <Icon
                                    size={15}
                                    className={`shrink-0 ${active ? 'text-primary' : 'text-base-content/45'}`}
                                />
                                <span className="min-w-0 flex-1 truncate text-xs font-medium text-base-content">
                                    {item.label}
                                </span>
                                {item.description && (
                                    <span className="ml-2 max-w-[55%] shrink-0 truncate text-right text-[11px] text-base-content/45">
                                        {item.description}
                                    </span>
                                )}
                            </button>
                        );
                    })}
                </div>
            )}
        </div>
    );
}
