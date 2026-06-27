import { useEffect, useState } from 'react';
import { Check, Edit2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../utils/cn';

export interface PreviewFile {
    title: string;
    content: string;
    originalContent: string;
}

interface EditablePreviewProps {
    files: PreviewFile[];
    onJsonChange: (title: string, value: unknown) => void;
}

export function parseEditablePreviewDraft(draft: string): { ok: true; value: unknown } | { ok: false; error: string } {
    try {
        return { ok: true, value: JSON.parse(draft) };
    } catch (error) {
        return { ok: false, error: error instanceof Error ? error.message : String(error) };
    }
}

export default function EditablePreview({ files, onJsonChange }: EditablePreviewProps) {
    const { t } = useTranslation();
    const [drafts, setDrafts] = useState<Record<string, string>>({});
    const [editing, setEditing] = useState<Record<string, boolean>>({});
    const [parseErrors, setParseErrors] = useState<Record<string, string | undefined>>({});

    useEffect(() => {
        setDrafts((prev) => {
            const next = { ...prev };
            for (const file of files) {
                // 中文注释（状态流转）：正在编辑时保留用户输入，避免后端预览刷新覆盖草稿；
                // 非编辑态才跟随结构化表单生成的新预览。
                if (!editing[file.title]) {
                    next[file.title] = file.content;
                }
            }
            return next;
        });
    }, [files, editing]);

    const handleDraftChange = (title: string, value: string) => {
        setDrafts(prev => ({ ...prev, [title]: value }));
        const parsed = parseEditablePreviewDraft(value);
        if (!parsed.ok) {
            setParseErrors(prev => ({ ...prev, [title]: parsed.error }));
            return;
        }
        setParseErrors(prev => ({ ...prev, [title]: undefined }));
        onJsonChange(title, parsed.value);
    };

    return (
        <div className="flex flex-col gap-4">
            {files.map((file) => {
                const stripComma = (s: string) => s.replace(/,\s*$/, '');
                const previewLines = file.content.split('\n');
                const originalSet = new Set(file.originalContent.split('\n').map(stripComma));
                const changedCount = previewLines.filter(line => !originalSet.has(stripComma(line))).length;
                const isEditing = !!editing[file.title];
                const draft = drafts[file.title] ?? file.content;
                const parseError = parseErrors[file.title];

                return (
                    <div key={file.title} className="relative rounded-md border border-gray-200 dark:border-slate-700 overflow-hidden bg-gray-50 dark:bg-[#1e1e2e]">
                        <div className="flex items-center justify-between bg-gray-100 dark:bg-slate-800/80 px-3 py-1.5 border-b border-gray-200 dark:border-slate-700">
                            <span className="text-xs font-mono text-gray-600 dark:text-slate-300">{file.title}</span>
                            <div className="flex items-center gap-2">
                                {changedCount > 0 && !isEditing && (
                                    <span className="text-[10px] font-medium text-emerald-600 dark:text-emerald-400/80 bg-emerald-500/10 px-1.5 py-0.5 rounded">
                                        {changedCount} {t('providers.previewChangedLines', '行变更')}
                                    </span>
                                )}
                                <button
                                    type="button"
                                    className="btn btn-ghost btn-xs gap-1"
                                    disabled={isEditing && !!parseError}
                                    onClick={() => setEditing(prev => ({ ...prev, [file.title]: !isEditing }))}
                                >
                                    {isEditing ? <Check className="w-3.5 h-3.5" /> : <Edit2 className="w-3.5 h-3.5" />}
                                    {isEditing ? t('common.confirm', '确认') : t('common.edit', '编辑')}
                                </button>
                            </div>
                        </div>

                        {isEditing ? (
                            <div className="p-2">
                                <textarea
                                    className={cn(
                                        "min-h-[220px] w-full resize-y rounded border bg-white px-3 py-2 font-mono text-xs leading-6 text-gray-900 outline-none focus:ring-1 dark:bg-slate-950 dark:text-slate-100",
                                        parseError
                                            ? "border-red-400 focus:ring-red-400"
                                            : "border-gray-300 focus:ring-blue-500 dark:border-slate-700"
                                    )}
                                    value={draft}
                                    spellCheck={false}
                                    onChange={(event) => handleDraftChange(file.title, event.target.value)}
                                />
                                {parseError && (
                                    <div className="mt-1 text-xs text-red-500">
                                        {t('providers.previewJsonError', 'JSON 解析失败，表单未更新')}：{parseError}
                                    </div>
                                )}
                            </div>
                        ) : (
                            <div className="p-0 overflow-x-auto">
                                {previewLines.map((line, lineIdx) => {
                                    const isNew = !originalSet.has(stripComma(line));

                                    return (
                                        <div
                                            key={lineIdx}
                                            className={cn(
                                                "flex font-mono text-[13px] leading-[1.7] border-l-2",
                                                isNew
                                                    ? "bg-emerald-500/10 border-l-emerald-400 text-emerald-700 dark:text-emerald-300"
                                                    : "border-l-transparent text-gray-800 dark:text-[#cdd6f4]"
                                            )}
                                        >
                                            <span className="select-none w-8 shrink-0 text-right pr-2 text-[11px] text-gray-400 dark:text-slate-600 leading-[1.7]">
                                                {lineIdx + 1}
                                            </span>
                                            <span className="px-3 whitespace-pre">{line || ' '}</span>
                                        </div>
                                    );
                                })}
                            </div>
                        )}
                    </div>
                );
            })}
        </div>
    );
}
