import {useCallback, useRef} from 'react';

/**
 * SVG icon strings (inlined to avoid React rendering inside contenteditable).
 * Sourced from lucide `File` and `Folder` icons at 13×13.
 */
const FILE_ICON_SVG = `<svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/></svg>`;
const FOLDER_ICON_SVG = `<svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/></svg>`;

const FILE_TAG_CLASS = 'file-tag';

/** Check whether a DOM node is a file-tag chip. */
export function isFileTag(node: Node | null): node is HTMLElement {
    return (
        node instanceof HTMLElement &&
        node.classList.contains(FILE_TAG_CLASS)
    );
}

/** Build a file-tag chip DOM element (not a React component). */
export function createFileTagElement(
    filePath: string,
    displayName: string,
    isDir: boolean,
): HTMLSpanElement {
    const tag = document.createElement('span');
    tag.className = FILE_TAG_CLASS;
    tag.contentEditable = 'false';
    tag.dataset.filePath = filePath;
    tag.setAttribute('title', filePath);

    // icon
    const icon = document.createElement('span');
    icon.className = 'file-tag-icon';
    icon.innerHTML = isDir ? FOLDER_ICON_SVG : FILE_ICON_SVG;
    tag.appendChild(icon);

    // text
    const text = document.createElement('span');
    text.className = 'file-tag-text';
    text.textContent = displayName;
    tag.appendChild(text);

    // close button
    const close = document.createElement('span');
    close.className = 'file-tag-close';
    close.textContent = '\u00d7'; // ×
    tag.appendChild(close);

    return tag;
}

/**
 * Extract the basename from a file path (last segment).
 */
function basename(path: string): string {
    const parts = path.split(/[/\\]/).filter(Boolean);
    return parts[parts.length - 1] ?? path;
}

/**
 * Walk the DOM tree of a contenteditable element and extract
 * plain text, converting file-tag chips back to `@filepath`.
 */
export function getPlainText(editor: HTMLElement): string {
    const parts: string[] = [];

    function walk(node: Node) {
        if (isFileTag(node)) {
            const filePath = (node as HTMLElement).dataset.filePath;
            if (filePath) {
                parts.push(`@${filePath}`);
            }
            return; // don't recurse into chip internals
        }

        if (node.nodeType === Node.TEXT_NODE) {
            parts.push(node.textContent ?? '');
            return;
        }

        // <br> → newline
        if (node.nodeName === 'BR') {
            parts.push('\n');
            return;
        }

        // Block elements (div, p) → newline before content (except first child)
        const isBlock =
            node.nodeName === 'DIV' ||
            node.nodeName === 'P';

        if (isBlock && node.previousSibling) {
            parts.push('\n');
        }

        node.childNodes.forEach(walk);
    }

    editor.childNodes.forEach(walk);
    return parts.join('');
}

/**
 * Compute a caret offset (in plain-text coordinates) from the current
 * Selection within the contenteditable element.
 *
 * Uses a recursive DOM walk: accumulates plain-text length for each node
 * until we reach the node/offset pair that the Selection points to.
 */
export function getCaretOffset(editor: HTMLElement): number {
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return 0;

    const range = selection.getRangeAt(0);
    const caretNode = range.startContainer;
    const caretChildOffset = range.startOffset;

    let offset = 0;
    let found = false;

    function walk(node: Node): void {
        if (found) return;

        // ── file-tag chip (atomic) ──
        if (isFileTag(node)) {
            // If the caret somehow ended up inside the chip, treat as "before"
            if (node === caretNode || node.contains(caretNode)) {
                found = true;
                return;
            }
            const fp = (node as HTMLElement).dataset.filePath ?? '';
            offset += `@${fp}`.length;
            return; // don't recurse into chip internals
        }

        // ── text node ──
        if (node.nodeType === Node.TEXT_NODE) {
            if (node === caretNode) {
                offset += caretChildOffset;
                found = true;
                return;
            }
            offset += (node.textContent ?? '').length;
            return;
        }

        // ── <br> ──
        if (node.nodeName === 'BR') {
            offset += 1;
            return;
        }

        // ── block element (div / p) ──
        if (
            (node.nodeName === 'DIV' || node.nodeName === 'P') &&
            node.previousSibling
        ) {
            offset += 1; // newline
        }

        // ── element whose childNodes contain the caret ──
        //    Selection.startContainer === this element,
        //    Selection.startOffset === index among children.
        if (node === caretNode) {
            // Count plain-text length of children [0, caretChildOffset)
            const children = node.childNodes;
            for (let i = 0; i < caretChildOffset && i < children.length; i++) {
                walk(children[i]);
                if (found) return; // shouldn't happen, but be safe
            }
            found = true;
            return;
        }

        // Recurse into children
        const children = node.childNodes;
        for (let i = 0; i < children.length; i++) {
            walk(children[i]);
            if (found) return;
        }
    }

    // Walk children of editor (NOT editor itself, to avoid spurious block-newline
    // from editor.previousSibling which is a DOM sibling like the resize button).
    if (editor === caretNode) {
        // Caret is at a child index of the editor itself
        const children = editor.childNodes;
        for (let i = 0; i < caretChildOffset && i < children.length; i++) {
            walk(children[i]);
            if (found) break;
        }
    } else {
        const children = editor.childNodes;
        for (let i = 0; i < children.length; i++) {
            walk(children[i]);
            if (found) break;
        }
    }
    return offset;
}

/**
 * Place the caret at a given plain-text offset within the contenteditable.
 */
export function setCaretOffset(editor: HTMLElement, targetOffset: number): void {
    let remaining = targetOffset;

    function findPosition(node: Node): { node: Node; offset: number } | null {
        if (isFileTag(node)) {
            const filePath = (node as HTMLElement).dataset.filePath ?? '';
            const len = `@${filePath}`.length;
            if (remaining <= len) {
                remaining = 0;
                // Place caret after this element
                return null; // handled specially
            }
            remaining -= len;
            return null;
        }

        if (node.nodeType === Node.TEXT_NODE) {
            const text = node.textContent ?? '';
            if (remaining <= text.length) {
                const result = { node, offset: remaining };
                remaining = 0;
                return result;
            }
            remaining -= text.length;
            return null;
        }

        if (node.nodeName === 'BR') {
            if (remaining <= 1) {
                remaining = 0;
                return null;
            }
            remaining -= 1;
            return null;
        }

        if (
            (node.nodeName === 'DIV' || node.nodeName === 'P') &&
            node.previousSibling
        ) {
            if (remaining <= 1) {
                remaining = 0;
                return { node: node, offset: 0 };
            }
            remaining -= 1;
        }

        for (const child of Array.from(node.childNodes)) {
            const result = findPosition(child);
            if (result || remaining <= 0) return result;
        }

        return null;
    }

    // Walk children of editor (NOT editor itself, to avoid spurious block-newline).
    let pos: { node: Node; offset: number } | null = null;
    for (const child of Array.from(editor.childNodes)) {
        pos = findPosition(child);
        if (pos || remaining <= 0) break;
    }
    const selection = window.getSelection();
    if (!selection) return;

    const range = document.createRange();
    if (pos) {
        range.setStart(pos.node, pos.offset);
    } else {
        // Place at end
        range.selectNodeContents(editor);
        range.collapse(false);
    }
    range.collapse(true);
    selection.removeAllRanges();
    selection.addRange(range);
}

/**
 * Insert a file-tag chip at the current caret position, replacing
 * the trigger text (`@query`).
 *
 * @param editor   The contenteditable element
 * @param triggerStart  Offset (in plain text) of the `@` trigger character
 * @param queryLength   Length of the query after `@`
 * @param filePath      Full relative path of the file
 * @param isDir         Whether the path is a directory
 */
export function insertFileTag(
    editor: HTMLElement,
    triggerStart: number,
    queryLength: number,
    filePath: string,
    isDir: boolean,
): void {
    const displayName = basename(filePath);
    const tag = createFileTagElement(filePath, displayName, isDir);

    // We need to remove the `@query` text and insert the tag in its place
    // Strategy: find the text node and offset for triggerStart,
    // then delete triggerStart..(triggerStart + 1 + queryLength) chars,
    // and insert the tag element.

    const removeLen = 1 + queryLength; // `@` + query

    // Rebuild approach: walk the DOM to find the right position
    const selection = window.getSelection();
    if (!selection) return;

    // Find the range to delete
    let charCount = 0;
    let startNode: Node | null = null;
    let startOffset = 0;
    let endNode: Node | null = null;
    let endOffset = 0;

    function findPositions(node: Node): boolean {
        if (isFileTag(node)) {
            const fp = (node as HTMLElement).dataset.filePath ?? '';
            charCount += `@${fp}`.length;
            return charCount >= triggerStart + removeLen;
        }

        if (node.nodeType === Node.TEXT_NODE) {
            const text = node.textContent ?? '';
            const nodeStart = charCount;
            const nodeEnd = charCount + text.length;

            // Start position
            if (!startNode && triggerStart >= nodeStart && triggerStart <= nodeEnd) {
                startNode = node;
                startOffset = triggerStart - nodeStart;
            }

            // End position
            if (!endNode && (triggerStart + removeLen) >= nodeStart && (triggerStart + removeLen) <= nodeEnd) {
                endNode = node;
                endOffset = (triggerStart + removeLen) - nodeStart;
                return true;
            }

            charCount += text.length;
            return false;
        }

        if (node.nodeName === 'BR') {
            charCount += 1;
            return false;
        }

        if (
            (node.nodeName === 'DIV' || node.nodeName === 'P') &&
            node.previousSibling
        ) {
            charCount += 1;
        }

        for (const child of Array.from(node.childNodes)) {
            if (findPositions(child)) return true;
        }
        return false;
    }

    // Walk children of editor (NOT editor itself, to avoid spurious block-newline
    // from editor.previousSibling).
    for (const child of Array.from(editor.childNodes)) {
        if (findPositions(child)) break;
    }

    if (!startNode || !endNode) return;

    const range = document.createRange();
    range.setStart(startNode, startOffset);
    range.setEnd(endNode, endOffset);
    range.deleteContents();

    // Insert the tag + a trailing space (so caret has somewhere to land)
    const trailingSpace = document.createTextNode('\u00a0'); // &nbsp; for whitespace
    range.insertNode(trailingSpace);
    range.insertNode(tag);

    // Place caret after the trailing space
    const newRange = document.createRange();
    newRange.setStartAfter(trailingSpace);
    newRange.collapse(true);
    selection.removeAllRanges();
    selection.addRange(newRange);
}

/**
 * Remove a file-tag chip from the editor and restore focus.
 */
export function removeFileTag(editor: HTMLElement, tag: HTMLElement): void {
    const selection = window.getSelection();
    const range = document.createRange();

    // If the next sibling is a single space/nbsp, remove it too
    const next = tag.nextSibling;
    if (
        next &&
        next.nodeType === Node.TEXT_NODE &&
        (next.textContent === '\u00a0' || next.textContent === ' ')
    ) {
        next.remove();
    }

    tag.remove();
    editor.focus();

    // Place caret at end
    range.selectNodeContents(editor);
    range.collapse(false);
    selection?.removeAllRanges();
    selection?.addRange(range);
}

/**
 * Custom hook returning refs and utility functions for contenteditable management.
 */
export function useContentEditable() {
    const editorRef = useRef<HTMLDivElement>(null);
    const composingRef = useRef(false);

    const getText = useCallback((): string => {
        const el = editorRef.current;
        if (!el) return '';
        return getPlainText(el);
    }, []);

    const getCaretPos = useCallback((): number => {
        const el = editorRef.current;
        if (!el) return 0;
        return getCaretOffset(el);
    }, []);

    const setCaretPos = useCallback((offset: number) => {
        const el = editorRef.current;
        if (!el) return;
        setCaretOffset(el, offset);
    }, []);

    const insertTag = useCallback(
        (triggerStart: number, queryLength: number, filePath: string, isDir: boolean) => {
            const el = editorRef.current;
            if (!el) return;
            insertFileTag(el, triggerStart, queryLength, filePath, isDir);
        },
        [],
    );

    const clearEditor = useCallback(() => {
        const el = editorRef.current;
        if (!el) return;
        el.innerHTML = '';
    }, []);

    const setEditorText = useCallback((text: string) => {
        const el = editorRef.current;
        if (!el) return;
        // Simple: set as text (no chips)
        el.textContent = text;
        // Place caret at end
        const range = document.createRange();
        range.selectNodeContents(el);
        range.collapse(false);
        const sel = window.getSelection();
        sel?.removeAllRanges();
        sel?.addRange(range);
    }, []);

    const focus = useCallback(() => {
        editorRef.current?.focus();
    }, []);

    return {
        editorRef,
        composingRef,
        getText,
        getCaretPos,
        setCaretPos,
        insertTag,
        clearEditor,
        setEditorText,
        focus,
    };
}
