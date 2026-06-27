import type {ChatMessage, ContentBlock} from '../types/chat';
import {
    getRenderableContentBlocks,
    isProtocolContextText,
    shouldRenderChatMessage,
    TOOL_RESULT_CONTENT,
} from './chatMessageFlow';
import {
    getImageBlockPreviewText,
    getImageBlockSearchText,
    isImageContentBlock,
    isImagePlaceholderText,
} from './chatImageBlocks';

export interface RenderableMessage {
    message: ChatMessage;
    originalIndex: number;
}

export interface RenderableMessageWindow {
    renderableMessages: RenderableMessage[];
    hiddenRenderableCount: number;
    totalRenderableCount: number;
}

export type AnchorPreviewKind = 'text' | 'image' | 'mixed' | 'empty';

export interface AnchorPreview {
    label: string;
    kind: AnchorPreviewKind;
}

export function getBlockSearchText(block: ContentBlock): string {
    if (isImageContentBlock(block)) return getImageBlockSearchText(block);
    if (block.type === 'text') return block.text;
    if (block.type === 'thinking') return block.thinking;
    if (block.type === 'tool_use') return `${block.name} ${JSON.stringify(block.input)}`;
    if (block.type === 'tool_result') {
        if (typeof block.content === 'string') return block.content;
        if (Array.isArray(block.content)) return block.content.map(getBlockSearchText).join('\n');
    }

    return '';
}

function getMessageContentSearchText(message: ChatMessage): string {
    if (typeof message.content !== 'string') return '';
    const blocks = getRenderableContentBlocks(message.raw);
    if (!blocks.some(isImageContentBlock)) return message.content;

    return message.content
        .split(/\r?\n/)
        .filter((line) => !isImagePlaceholderText(line))
        .join('\n');
}

export function getMessageSearchText(message: ChatMessage): string {
    const rawText = getRenderableContentBlocks(message.raw)
        .map(getBlockSearchText)
        .join('\n');
    const contentText = getMessageContentSearchText(message);

    return [message.role, contentText, rawText, message.error]
        .filter((part): part is string => Boolean(part))
        .join('\n')
        .toLowerCase();
}

export function getRenderableMessages(messages: ChatMessage[]): RenderableMessage[] {
    return messages
        .map((message, originalIndex) => ({ message, originalIndex }))
        .filter(({ message }) => shouldRenderChatMessage(message));
}

export function getRecentRenderableMessages(
    messages: ChatMessage[],
    visibleCount: number,
): RenderableMessageWindow {
    const maxVisible = Math.max(0, Math.floor(visibleCount));
    const renderableMessages: RenderableMessage[] = [];
    let hiddenRenderableCount = 0;

    for (let index = messages.length - 1; index >= 0; index -= 1) {
        const message = messages[index];
        if (!shouldRenderChatMessage(message)) continue;

        if (renderableMessages.length < maxVisible) {
            renderableMessages.push({ message, originalIndex: index });
            continue;
        }

        hiddenRenderableCount += 1;
    }

    renderableMessages.reverse();

    return {
        renderableMessages,
        hiddenRenderableCount,
        totalRenderableCount: hiddenRenderableCount + renderableMessages.length,
    };
}

export function filterRenderableMessages(
    renderableMessages: RenderableMessage[],
    normalizedSearchQuery: string,
): RenderableMessage[] {
    if (!normalizedSearchQuery) return renderableMessages;

    return renderableMessages.filter(({ message }) => (
        getMessageSearchText(message).includes(normalizedSearchQuery)
    ));
}

interface SearchStatusContextOptions {
    maxMessages?: number;
    contextAfterMatch?: number;
}

const DEFAULT_SEARCH_STATUS_CONTEXT_MAX_MESSAGES = 80;
const DEFAULT_SEARCH_STATUS_CONTEXT_AFTER_MATCH = 16;

export function getSearchStatusContextMessages(
    sourceMessages: ChatMessage[],
    matchedMessages: RenderableMessage[],
    options: SearchStatusContextOptions = {},
): ChatMessage[] {
    if (sourceMessages.length === 0 || matchedMessages.length === 0) return [];

    const maxMessages = Math.max(
        0,
        Math.floor(options.maxMessages ?? DEFAULT_SEARCH_STATUS_CONTEXT_MAX_MESSAGES),
    );
    if (maxMessages === 0) return [];

    const contextAfterMatch = Math.max(
        1,
        Math.floor(options.contextAfterMatch ?? DEFAULT_SEARCH_STATUS_CONTEXT_AFTER_MATCH),
    );
    const selectedIndexes = new Set<number>();
    const sortedMatches = [...matchedMessages]
        .sort((a, b) => a.originalIndex - b.originalIndex);

    for (const {originalIndex} of sortedMatches) {
        if (selectedIndexes.size >= maxMessages) break;

        const startIndex = Math.max(0, Math.min(sourceMessages.length - 1, originalIndex));
        const endIndex = Math.min(sourceMessages.length, startIndex + contextAfterMatch);
        for (let index = startIndex; index < endIndex && selectedIndexes.size < maxMessages; index += 1) {
            selectedIndexes.add(index);
        }
    }

    return [...selectedIndexes]
        .sort((a, b) => a - b)
        .map((index) => sourceMessages[index]);
}

export function getVisibleAnchorMessages(
    renderableMessages: RenderableMessage[],
    collapsedCount: number,
): RenderableMessage[] {
    return renderableMessages.slice(Math.max(0, collapsedCount));
}

function compactPreviewText(text: string, maxLength: number): string {
    const normalized = text.replace(/\s+/g, ' ').trim();
    if (normalized.length <= maxLength) return normalized;
    return `${normalized.slice(0, maxLength)}...`;
}

function getAnchorBlockPreviewText(block: ContentBlock): string {
    if (isImageContentBlock(block)) return getImageBlockPreviewText(block);
    if (block.type === 'text') return block.text.replace(/\s+/g, ' ').trim();
    if (block.type === 'thinking') return block.thinking.replace(/\s+/g, ' ').trim();
    return '';
}

function getAnchorPreviewKind(blocks: ContentBlock[], rawText: string): AnchorPreviewKind {
    const hasImage = blocks.some(isImageContentBlock);
    const hasText = blocks.some((block) => (
        (block.type === 'text' && block.text.trim().length > 0)
        || (block.type === 'thinking' && block.thinking.trim().length > 0)
    ));

    if (hasImage && hasText) return 'mixed';
    if (hasImage) return 'image';
    if (hasText || rawText.trim().length > 0) return 'text';
    return 'empty';
}

export function getAnchorPreview(
    message: ChatMessage,
    fallbackLabel: string,
    maxLength = 72,
): AnchorPreview {
    const blocks = getRenderableContentBlocks(message.raw);
    const rawPreview = blocks
        .map(getAnchorBlockPreviewText)
        .filter((text) => text.trim().length > 0)
        .join(' · ');
    const contentPreview = typeof message.content === 'string'
        ? message.content.replace(/\s+/g, ' ').trim()
        : '';
    const rawText = rawPreview || contentPreview;
    const kind = getAnchorPreviewKind(blocks, contentPreview);

    return {
        label: rawText ? compactPreviewText(rawText, maxLength) : fallbackLabel,
        kind,
    };
}

export function getAnchorPreviewLabel(
    message: ChatMessage,
    fallbackLabel: string,
    maxLength = 72,
): string {
    return getAnchorPreview(message, fallbackLabel, maxLength).label;
}

export function isMessageAnchorCandidate(message: ChatMessage): boolean {
    if (message.role !== 'user') return false;
    if (!shouldRenderChatMessage(message)) return false;
    if (message.content.trim() === TOOL_RESULT_CONTENT) return false;
    if (getRenderableContentBlocks(message.raw).some((block) => (
        block.type === 'text' && isProtocolContextText(block.text)
    ))) return false;
    const preview = getAnchorPreview(message, '', 1);
    return preview.kind !== 'empty' && preview.label.trim().length > 0;
}
