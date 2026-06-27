import {Clock} from 'lucide-react';
import type {TokenUsage} from '../../types/chat';

interface MessageMetaProps {
    /** 本轮耗时（毫秒） */
    durationMs?: number;
    /** token 用量 */
    usage?: TokenUsage;
    /** 是否按 assistant 流式尾注风格展示 */
    compact?: boolean;
}

/** 格式化耗时：mm:ss 或 h:mm:ss */
function formatDuration(durationMs: number): string {
    const seconds = Math.max(0, Math.floor(durationMs / 1000));
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const remainder = seconds % 60;
    if (hours > 0) {
        return `${hours}:${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`;
    }
    return `${minutes}:${String(remainder).padStart(2, '0')}`;
}

/** 格式化 token 数：1234 → 1.2K */
function formatTokenCount(count: number): string {
    if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
    if (count >= 1_000) return `${(count / 1_000).toFixed(1)}K`;
    return String(count);
}

/**
 * 消息元数据 - 显示耗时和 token 用量（assistant 消息流式结束后）
 */
export default function MessageMeta({ durationMs, usage, compact = false }: MessageMetaProps) {
    if (durationMs === undefined && !usage) return null;

    // 计算总输入 token（非缓存输入 + 缓存写 + 缓存读）
    const totalInput = usage
        ? usage.input_tokens + usage.cache_creation_input_tokens + usage.cache_read_input_tokens
        : 0;
    const output = usage?.output_tokens ?? 0;
    const hasTokens = totalInput > 0 || output > 0;

    return (
        <div
            className={
                compact
                    ? 'inline-flex items-center gap-1 text-[11px] leading-none text-base-content/42'
                    : 'flex items-center gap-1.5 mt-1.5 text-xs text-base-content/50'
            }
        >
            {durationMs !== undefined && (
                <>
                    <Clock size={compact ? 10 : 12} className={compact ? 'opacity-70' : ''} />
                    <span>{compact ? '耗时' : '本次耗时'}</span>
                    <span className={compact ? 'font-medium text-base-content/60' : 'font-medium'}>
                        {formatDuration(durationMs)}
                    </span>
                </>
            )}
            {durationMs !== undefined && hasTokens && (
                <span className="opacity-40">·</span>
            )}
            {hasTokens && (
                <span
                    title={
                        usage
                            ? `输入 ${formatTokenCount(usage.input_tokens)} · 缓存写 ${formatTokenCount(usage.cache_creation_input_tokens)} · 缓存读 ${formatTokenCount(usage.cache_read_input_tokens)} · 输出 ${formatTokenCount(output)}`
                            : undefined
                    }
                >
                    {compact ? 'tokens' : '输入'} {formatTokenCount(totalInput)} / {compact ? 'out' : '输出'} {formatTokenCount(output)}
                </span>
            )}
        </div>
    );
}
