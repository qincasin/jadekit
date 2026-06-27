import {type RefObject, useCallback, useEffect, useMemo, useRef, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {AlertTriangle, Loader2, RefreshCw} from 'lucide-react';
import type {ChatMessage, ToolResultBlock} from '../../types/chat';
import {
    getClampedRevealState,
    getCollapsedMessageWindow,
    getEffectiveRevealedCount,
    getManualRevealWindow,
    getNextRevealState,
    getRevealStateAfterServerExpansion,
    getScrollTopAfterPrepend,
    REVEAL_PAGE_SIZE,
    shouldAutoRevealEarlierMessages,
    shouldLoadEarlierServerHistory,
    type TranscriptRevealState,
    VISIBLE_MESSAGE_WINDOW,
} from '../../utils/chatUiBehavior';
import {
    filterRenderableMessages,
    getRecentRenderableMessages,
    getRenderableMessages,
    isMessageAnchorCandidate,
    type RenderableMessage,
} from '../../utils/chatNavigation';
import {getContentBlocksFromRaw} from '../../utils/chatMessageFlow';
import MessageItem from './MessageItem';

interface MessageListProps {
    messages: ChatMessage[];
    searchQuery?: string;
    fullHistorySearchStatus?: 'loading' | 'complete' | 'error' | null;
    scrollContainerRef?: RefObject<HTMLDivElement | null>;
    onCollapsedCountChange?: (count: number) => void;
    onMessageNodeRef?: (messageId: string, node: HTMLElement | null) => void;
    onRetryFullHistorySearch?: () => void;
    hasEarlierServerHistory?: boolean;
    isLoadingEarlierServerHistory?: boolean;
    onLoadEarlierServerHistory?: () => void;
}

function collectToolResultsInRange(
    messages: ChatMessage[],
    startIndex: number,
    endIndex: number,
    results: Map<string, ToolResultBlock>,
) {
    for (let index = startIndex; index < endIndex; index += 1) {
        getContentBlocksFromRaw(messages[index].raw).forEach((block) => {
            if (block.type === 'tool_result' && !results.has(block.tool_use_id)) {
                results.set(block.tool_use_id, block);
            }
        });
    }
}

export default function MessageList({
    messages,
    searchQuery = '',
    fullHistorySearchStatus = null,
    scrollContainerRef,
    onCollapsedCountChange,
    onMessageNodeRef,
    onRetryFullHistorySearch,
    hasEarlierServerHistory = false,
    isLoadingEarlierServerHistory = false,
    onLoadEarlierServerHistory,
}: MessageListProps) {
    const { t } = useTranslation();
    const translateWithFallback = (key: string, fallback: string, options?: Record<string, unknown>) => {
        const translated = options ? t(key, options) : t(key);
        return translated === key ? fallback : translated;
    };
    const [revealState, setRevealState] = useState<TranscriptRevealState>({
        transcriptKey: '',
        revealedCount: 0,
    });
    const revealAnchorRef = useRef<{
        previousScrollHeight: number;
        previousScrollTop: number;
    } | null>(null);
    const revealPendingRef = useRef(false);
    const prevLastMessageIdRef = useRef<string | null>(null);
    const prevTranscriptKeyRef = useRef<string | null>(null);
    const prevMessagesLengthRef = useRef(0);
    const prevVisibleRenderableCountRef = useRef(0);
    const normalizedSearchQuery = searchQuery.trim().toLowerCase();
    const isSearching = normalizedSearchQuery.length > 0;
    const showFullHistorySearchLoading = isSearching && fullHistorySearchStatus === 'loading';
    const showFullHistorySearchError = isSearching && fullHistorySearchStatus === 'error';
    const formatSearchResultsLabel = (count: number) => translateWithFallback(
        'chat.layout.searchResults',
        `Found ${count} matching message${count === 1 ? '' : 's'}`,
        {count},
    );
    const searchNoResultsLabel = translateWithFallback(
        'chat.layout.searchNoResults',
        'No matching messages found',
    );
    const searchFullHistoryRetryLabel = translateWithFallback('chat.layout.searchFullHistoryRetry', 'Retry');
    const searchFullHistoryLoadingLabel = translateWithFallback(
        'chat.layout.searchFullHistoryLoading',
        'Searching complete history for older matches...',
    );
    const searchFullHistoryErrorLabel = translateWithFallback(
        'chat.layout.searchFullHistoryError',
        'Complete history search failed. Current results only cover the loaded window.',
    );
    const loadEarlierServerHistoryLabel = translateWithFallback(
        'chat.layout.loadEarlierServerHistory',
        'Load earlier history from this session',
    );
    const loadingEarlierServerHistoryLabel = translateWithFallback(
        'chat.layout.loadingEarlierServerHistory',
        'Loading earlier history...',
    );

    const transcriptKey = messages[0]?.id ?? '';
    const lastMessageId = messages.length > 0 ? messages[messages.length - 1].id : null;

    // 服务端补全完整历史时，messages 由窗口尾部（如 120 条）被替换为完整记录（如 5000 条），
    // 第一条消息 id 从窗口首条变为真正的首条。若不迁移 reveal 状态，transcriptKey 改变会让
    // revealedCount 归零、视图被甩回最近 15 条尾部。这里检测此类「前置扩展」并迁移 reveal 状态，
    // 让之前可见的消息保持可见，并额外多露出一页更早历史。
    if (!isSearching) {
        const expansion = getRevealStateAfterServerExpansion({
            prevLastMessageId: prevLastMessageIdRef.current,
            prevTranscriptKey: prevTranscriptKeyRef.current,
            prevMessagesLength: prevMessagesLengthRef.current,
            prevVisibleRenderableCount: prevVisibleRenderableCountRef.current,
            nextLastMessageId: lastMessageId,
            nextTranscriptKey: transcriptKey,
            nextMessagesLength: messages.length,
        });

        if (expansion.isServerExpansion && expansion.revealState) {
            // 同步更新 refs，避免迁移后的重渲染再次命中扩展判定造成循环。
            prevLastMessageIdRef.current = lastMessageId;
            prevTranscriptKeyRef.current = transcriptKey;
            prevMessagesLengthRef.current = messages.length;
            const migratedRevealState = expansion.revealState;
            if (revealState.transcriptKey !== migratedRevealState.transcriptKey
                || revealState.revealedCount !== migratedRevealState.revealedCount) {
                setRevealState(migratedRevealState);
            }
        }
    }

    const revealedCount = getEffectiveRevealedCount(revealState, transcriptKey);
    const requestedVisibleCount = VISIBLE_MESSAGE_WINDOW + revealedCount;
    const renderableWindow = useMemo(() => {
        if (isSearching) {
            const allRenderableMessages = getRenderableMessages(messages);
            return {
                renderableMessages: allRenderableMessages,
                hiddenRenderableCount: 0,
                totalRenderableCount: allRenderableMessages.length,
            };
        }

        return getRecentRenderableMessages(messages, requestedVisibleCount);
    }, [isSearching, messages, requestedVisibleCount]);
    const renderableMessages = renderableWindow.renderableMessages;

    const filteredMessages = useMemo<RenderableMessage[]>(() => {
        if (!isSearching) return renderableMessages;
        return filterRenderableMessages(renderableMessages, normalizedSearchQuery);
    }, [isSearching, normalizedSearchQuery, renderableMessages]);

    const {
        totalEarlierMessages,
        collapsedCount,
        nextRevealCount,
        visibleStartIndex,
    } = useMemo(() => {
        if (!isSearching) {
            const hiddenCount = renderableWindow.hiddenRenderableCount;
            return {
                ...getManualRevealWindow({
                    remainingHiddenCount: hiddenCount,
                    revealedCount,
                    pageSize: REVEAL_PAGE_SIZE,
                }),
                visibleStartIndex: 0,
            };
        }

        return getCollapsedMessageWindow({
            filteredCount: filteredMessages.length,
            revealedCount,
            isSearching,
        });
    }, [filteredMessages.length, isSearching, renderableWindow.hiddenRenderableCount, revealedCount]);
    const visibleMessages = isSearching
        ? filteredMessages.slice(visibleStartIndex)
        : filteredMessages;
    const showEarlierLabel = translateWithFallback(
        'chat.message.showEarlier',
        `${collapsedCount} earlier message${collapsedCount === 1 ? '' : 's'} ${collapsedCount === 1 ? 'is' : 'are'} collapsed. Click to load ${nextRevealCount} more`,
        {count: nextRevealCount, total: collapsedCount},
    );
    const lastRenderableIndex = renderableMessages.length > 0
        ? renderableMessages[renderableMessages.length - 1].originalIndex
        : undefined;
    const toolResultSearchStartIndex = visibleMessages[0]?.originalIndex ?? messages.length;
    const toolResultById = useMemo(() => {
        const results = new Map<string, ToolResultBlock>();

        collectToolResultsInRange(messages, toolResultSearchStartIndex, messages.length, results);
        collectToolResultsInRange(messages, 0, toolResultSearchStartIndex, results);

        return results;
    }, [messages, toolResultSearchStartIndex]);
    const findVisibleToolResult = useCallback((toolId: string | undefined): ToolResultBlock | null => {
        if (!toolId) return null;
        return toolResultById.get(toolId) ?? null;
    }, [toolResultById]);

    useEffect(() => {
        setRevealState((current) => getClampedRevealState(
            current,
            transcriptKey,
            totalEarlierMessages,
        ));
    }, [totalEarlierMessages, transcriptKey]);

    useEffect(() => {
        onCollapsedCountChange?.(collapsedCount);
    }, [collapsedCount, onCollapsedCountChange]);

    // 提交后记录本次渲染的快照，供下一次「前置扩展」检测使用。
    useEffect(() => {
        prevLastMessageIdRef.current = lastMessageId;
        prevTranscriptKeyRef.current = transcriptKey;
        prevMessagesLengthRef.current = messages.length;
        prevVisibleRenderableCountRef.current = renderableMessages.length;
    }, [lastMessageId, messages.length, renderableMessages.length, transcriptKey]);

    const revealEarlierMessages = useCallback((scrollEl?: HTMLDivElement | null) => {
        if (collapsedCount <= 0) return;

        if (scrollEl) {
            revealAnchorRef.current = {
                previousScrollHeight: scrollEl.scrollHeight,
                previousScrollTop: scrollEl.scrollTop,
            };
        }

        revealPendingRef.current = true;
        setRevealState((current) => {
            const currentRevealed = getEffectiveRevealedCount(current, transcriptKey);
            const next = getNextRevealState(current, transcriptKey, totalEarlierMessages);
            if (next.revealedCount === currentRevealed) {
                revealPendingRef.current = false;
                revealAnchorRef.current = null;
            }
            return next;
        });
    }, [collapsedCount, totalEarlierMessages, transcriptKey]);

    useEffect(() => {
        const scrollEl = scrollContainerRef?.current;
        const anchor = revealAnchorRef.current;
        if (!scrollEl || !anchor) {
            revealPendingRef.current = false;
            return;
        }

        requestAnimationFrame(() => {
            scrollEl.scrollTop = getScrollTopAfterPrepend({
                previousScrollTop: anchor.previousScrollTop,
                previousScrollHeight: anchor.previousScrollHeight,
                nextScrollHeight: scrollEl.scrollHeight,
            });
            revealAnchorRef.current = null;
            revealPendingRef.current = false;
        });
    }, [scrollContainerRef, visibleMessages.length]);

    const triggerLoadEarlierServerHistory = useCallback((scrollEl?: HTMLDivElement | null) => {
        // 在 messages 增长前捕获滚动锚点，使完整历史前置插入后视口停在原来的顶部消息。
        if (scrollEl) {
            revealAnchorRef.current = {
                previousScrollHeight: scrollEl.scrollHeight,
                previousScrollTop: scrollEl.scrollTop,
            };
            revealPendingRef.current = true;
        }
        onLoadEarlierServerHistory?.();
    }, [onLoadEarlierServerHistory]);

    const handleRevealEarlierMessages = () => {
        if (revealPendingRef.current || isSearching) return;
        if (collapsedCount > 0) {
            revealEarlierMessages(scrollContainerRef?.current);
            return;
        }
        if (hasEarlierServerHistory && !isLoadingEarlierServerHistory) {
            triggerLoadEarlierServerHistory(scrollContainerRef?.current);
        }
    };

    const handleAutoRevealScroll = useCallback(() => {
        const scrollEl = scrollContainerRef?.current;
        if (!scrollEl) return;

        if (shouldAutoRevealEarlierMessages({
            scrollTop: scrollEl.scrollTop,
            collapsedCount,
            isSearching,
            revealPending: revealPendingRef.current,
        })) {
            revealEarlierMessages(scrollEl);
            return;
        }

        if (revealPendingRef.current) return;

        if (shouldLoadEarlierServerHistory({
            scrollTop: scrollEl.scrollTop,
            collapsedCount,
            isSearching,
            hasEarlierServerHistory,
            isLoadingEarlierServerHistory,
        })) {
            triggerLoadEarlierServerHistory(scrollEl);
        }
    }, [
        collapsedCount,
        hasEarlierServerHistory,
        isLoadingEarlierServerHistory,
        isSearching,
        revealEarlierMessages,
        scrollContainerRef,
        triggerLoadEarlierServerHistory,
    ]);

    useEffect(() => {
        const scrollEl = scrollContainerRef?.current;
        if (!scrollEl || isSearching) return undefined;
        if (collapsedCount <= 0 && !hasEarlierServerHistory) return undefined;

        scrollEl.addEventListener('scroll', handleAutoRevealScroll, {passive: true});
        return () => scrollEl.removeEventListener('scroll', handleAutoRevealScroll);
    }, [collapsedCount, handleAutoRevealScroll, hasEarlierServerHistory, isSearching, scrollContainerRef]);

    return (
        <div className="chat-message-list space-y-1 pb-6">
            {normalizedSearchQuery && (
                <div className="mx-auto w-full rounded-lg border border-base-300 bg-base-100/80 px-3 py-2 text-xs text-base-content/60 shadow-sm">
                    <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
                        <div className="flex min-w-0 items-center gap-2">
                            {showFullHistorySearchLoading && (
                                <Loader2 size={13} className="flex-shrink-0 animate-spin text-info" />
                            )}
                            {showFullHistorySearchError && (
                                <AlertTriangle size={13} className="flex-shrink-0 text-warning" />
                            )}
                            <span className="min-w-0 truncate">
                                {filteredMessages.length > 0
                                    ? formatSearchResultsLabel(filteredMessages.length)
                                    : searchNoResultsLabel}
                            </span>
                        </div>
                        {showFullHistorySearchError && onRetryFullHistorySearch && (
                            <button
                                type="button"
                                className="btn btn-ghost btn-xs h-6 min-h-0 gap-1 px-2 text-warning"
                                aria-label={searchFullHistoryRetryLabel}
                                onClick={onRetryFullHistorySearch}
                            >
                                <RefreshCw size={12} />
                                <span>{searchFullHistoryRetryLabel}</span>
                            </button>
                        )}
                    </div>
                    {(showFullHistorySearchLoading || showFullHistorySearchError) && (
                        <div className="mt-1 text-[11px] leading-snug text-base-content/45">
                            {showFullHistorySearchLoading
                                ? searchFullHistoryLoadingLabel
                                : searchFullHistoryErrorLabel}
                        </div>
                    )}
                </div>
            )}

            {collapsedCount > 0 && (
                <div className="mx-auto flex w-full justify-center py-1">
                    <button
                        type="button"
                        className="rounded-full border border-base-300 bg-base-100/80 px-3 py-1 text-[11px] text-base-content/55 shadow-sm backdrop-blur transition-colors hover:border-primary/35 hover:text-primary focus:outline-none focus:ring-2 focus:ring-primary/25"
                        title={showEarlierLabel}
                        aria-label={showEarlierLabel}
                        onClick={handleRevealEarlierMessages}
                    >
                        {showEarlierLabel}
                    </button>
                </div>
            )}

            {!isSearching && collapsedCount <= 0 && hasEarlierServerHistory && (
                <div className="mx-auto flex w-full justify-center py-1">
                    <button
                        type="button"
                        className="flex items-center gap-1 rounded-full border border-base-300 bg-base-100/80 px-3 py-1 text-[11px] text-base-content/55 shadow-sm backdrop-blur transition-colors hover:border-primary/35 hover:text-primary focus:outline-none focus:ring-2 focus:ring-primary/25 disabled:cursor-not-allowed disabled:opacity-60"
                        title={isLoadingEarlierServerHistory ? loadingEarlierServerHistoryLabel : loadEarlierServerHistoryLabel}
                        aria-label={isLoadingEarlierServerHistory ? loadingEarlierServerHistoryLabel : loadEarlierServerHistoryLabel}
                        disabled={isLoadingEarlierServerHistory}
                        onClick={handleRevealEarlierMessages}
                    >
                        {isLoadingEarlierServerHistory && (
                            <Loader2 size={11} className="animate-spin" />
                        )}
                        {isLoadingEarlierServerHistory ? loadingEarlierServerHistoryLabel : loadEarlierServerHistoryLabel}
                    </button>
                </div>
            )}

            {visibleMessages.map(({ message, originalIndex }) => (
                <MessageItem
                    key={message.id}
                    message={message}
                    isLast={originalIndex === lastRenderableIndex}
                    isSearchMatch={isSearching}
                    anchorId={isMessageAnchorCandidate(message) ? message.id : undefined}
                    onAnchorRef={onMessageNodeRef}
                    findToolResult={findVisibleToolResult}
                />
            ))}
        </div>
    );
}
