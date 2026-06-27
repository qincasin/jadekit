import {marked} from 'marked';
import DOMPurify from 'dompurify';
import {memo, useEffect, useMemo, useRef} from 'react';
import {useTranslation} from 'react-i18next';
import hljs from 'highlight.js/lib/core';
import {markedHighlight} from 'marked-highlight';

// 导入常用语言
import bash from 'highlight.js/lib/languages/bash';
import css from 'highlight.js/lib/languages/css';
import diff from 'highlight.js/lib/languages/diff';
import go from 'highlight.js/lib/languages/go';
import java from 'highlight.js/lib/languages/java';
import javascript from 'highlight.js/lib/languages/javascript';
import json from 'highlight.js/lib/languages/json';
import kotlin from 'highlight.js/lib/languages/kotlin';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import sql from 'highlight.js/lib/languages/sql';
import typescript from 'highlight.js/lib/languages/typescript';
import xml from 'highlight.js/lib/languages/xml';
import yaml from 'highlight.js/lib/languages/yaml';

// 导入样式：以 github(浅色) 为基底，深色主题下由 App.css 的
// [data-theme="dark"] .hljs 覆盖为 github-dark 调色板，使代码块跟随主题。
import 'highlight.js/styles/github.css';

// 注册语言
const languages = [
    ['bash', bash],
    ['css', css],
    ['diff', diff],
    ['go', go],
    ['java', java],
    ['javascript', javascript],
    ['json', json],
    ['kotlin', kotlin],
    ['python', python],
    ['rust', rust],
    ['sql', sql],
    ['typescript', typescript],
    ['xml', xml],
    ['yaml', yaml],
] as const;

languages.forEach(([name, lang]) => {
    hljs.registerLanguage(name, lang);
});

// 注册别名
hljs.registerAliases(['js', 'jsx'], { languageName: 'javascript' });
hljs.registerAliases(['ts', 'tsx'], { languageName: 'typescript' });
hljs.registerAliases(['sh', 'zsh'], { languageName: 'bash' });
hljs.registerAliases(['html'], { languageName: 'xml' });
hljs.registerAliases(['yml'], { languageName: 'yaml' });

// 配置 marked 使用语法高亮
marked.use(
    markedHighlight({
        highlight(code: string, lang: string) {
            if (lang && hljs.getLanguage(lang)) {
                try {
                    return hljs.highlight(code, { language: lang }).value;
                } catch {
                    // Fall through
                }
            }
            return hljs.highlightAuto(code).value;
        },
    })
);

// 配置 marked 选项
marked.setOptions({
    gfm: true, // GitHub Flavored Markdown
    breaks: true, // 换行符转换为 <br>
});

interface MarkdownBlockProps {
    content: string;
    isStreaming?: boolean;
}

const SAFE_LINK_PROTOCOLS = new Set(['http:', 'https:', 'mailto:', 'file:']);
const CONTROL_CHARACTER_PATTERN = /[\u0000-\u001F\u007F]/;
const URL_SCHEME_PATTERN = /^[a-zA-Z][a-zA-Z\d+.-]*:/;
const WINDOWS_DRIVE_PATH_PATTERN = /^[a-zA-Z]:[\\/]/;

function isSafeHref(href: string): boolean {
    const trimmedHref = href.trim();

    if (!trimmedHref || CONTROL_CHARACTER_PATTERN.test(trimmedHref)) {
        return false;
    }

    if (
        trimmedHref.startsWith('#')
        || trimmedHref.startsWith('/')
        || trimmedHref.startsWith('./')
        || trimmedHref.startsWith('../')
        || WINDOWS_DRIVE_PATH_PATTERN.test(trimmedHref)
    ) {
        return true;
    }

    if (!URL_SCHEME_PATTERN.test(trimmedHref)) {
        return true;
    }

    try {
        return SAFE_LINK_PROTOCOLS.has(new URL(trimmedHref).protocol);
    } catch {
        return false;
    }
}

function sanitizeMarkdownHtml(rawHtml: string): string {
    const sanitizedHtml = DOMPurify.sanitize(rawHtml, {
        ALLOW_UNKNOWN_PROTOCOLS: true,
    });

    if (typeof document === 'undefined') {
        return sanitizedHtml;
    }

    const template = document.createElement('template');
    template.innerHTML = sanitizedHtml;

    template.content.querySelectorAll('a[href]').forEach((link) => {
        const href = link.getAttribute('href');

        if (!href || !isSafeHref(href)) {
            link.removeAttribute('href');
            return;
        }

        if (/^https?:/i.test(href)) {
            link.setAttribute('target', '_blank');
            link.setAttribute('rel', 'noopener noreferrer');
        }
    });

    return template.innerHTML;
}

function escapeHtml(text: string): string {
    return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function translateWithFallback(t: (key: string) => string, key: string, fallback: string): string {
    const translated = t(key);
    return translated === key ? fallback : translated;
}

export function getMarkdownCodeCopyLabels(t: (key: string) => string) {
    return {
        copyCodeLabel: translateWithFallback(t, 'chat.markdown.copyCode', 'Copy code'),
        copiedCodeLabel: translateWithFallback(t, 'chat.markdown.copiedCode', 'Copied code'),
    };
}

/**
 * Markdown 渲染组件
 * 支持代码高亮、GFM、代码复制按钮
 */
function MarkdownBlock({ content, isStreaming = false }: MarkdownBlockProps) {
    const { t } = useTranslation();
    const containerRef = useRef<HTMLDivElement>(null);
    const {copyCodeLabel, copiedCodeLabel} = useMemo(() => getMarkdownCodeCopyLabels(t), [t]);

    // 渲染 Markdown
    const html = useMemo(() => {
        let markdown = content;

        // 流式渲染：自动补全未闭合的代码块
        if (isStreaming && content.includes('```')) {
            const openCount = (content.match(/```/g) || []).length;
            if (openCount % 2 === 1) {
                markdown = content + '\n```';
            }
        }

        try {
            const rawHtml = marked.parse(markdown) as string;
            return sanitizeMarkdownHtml(rawHtml);
        } catch (e) {
            console.error('[MarkdownBlock] Parse error:', e);
            return escapeHtml(content);
        }
    }, [content, isStreaming]);

    // 添加复制按钮到代码块
    useEffect(() => {
        if (!containerRef.current) return;

        const cleanupCallbacks: Array<() => void> = [];
        const codeBlocks = containerRef.current.querySelectorAll('pre > code');
        codeBlocks.forEach((codeBlock) => {
            const pre = codeBlock.parentElement;
            if (!pre || pre.querySelector('.copy-button')) return;

            // 创建复制按钮
            const button = document.createElement('button');
            button.className = 'copy-button';
            button.innerHTML = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>`;
            button.title = copyCodeLabel;
            button.setAttribute('aria-label', copyCodeLabel);

            let resetTimer: number | null = null;
            const handleCopy = async () => {
                const code = codeBlock.textContent || '';
                try {
                    await navigator.clipboard.writeText(code);
                    button.innerHTML = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"></polyline></svg>`;
                    button.classList.add('copied');
                    button.title = copiedCodeLabel;
                    button.setAttribute('aria-label', copiedCodeLabel);
                    resetTimer = window.setTimeout(() => {
                        button.innerHTML = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>`;
                        button.classList.remove('copied');
                        button.title = copyCodeLabel;
                        button.setAttribute('aria-label', copyCodeLabel);
                    }, 2000);
                } catch (e) {
                    console.error('[MarkdownBlock] Copy failed:', e);
                }
            };

            button.addEventListener('click', handleCopy);

            pre.style.position = 'relative';
            pre.appendChild(button);
            cleanupCallbacks.push(() => {
                button.removeEventListener('click', handleCopy);
                if (resetTimer !== null) {
                    window.clearTimeout(resetTimer);
                }
            });
        });

        return () => cleanupCallbacks.forEach((cleanup) => cleanup());
    }, [copyCodeLabel, copiedCodeLabel, html]);

    return (
        <div
            ref={containerRef}
            className="markdown-block"
            dangerouslySetInnerHTML={{ __html: html }}
        />
    );
}

export default memo(MarkdownBlock);
