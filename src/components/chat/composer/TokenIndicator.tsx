import { useTranslation } from 'react-i18next';

interface TokenIndicatorProps {
    /** 0-100 */
    percentage: number;
    usedTokens?: number;
    maxTokens?: number;
    size?: number;
}

function formatTokens(value?: number): string | undefined {
    if (typeof value !== 'number' || !isFinite(value)) return undefined;
    if (value >= 1000) {
        const k = value / 1000;
        return Number.isInteger(k) ? `${k}k` : `${k.toFixed(1)}k`;
    }
    return `${value}`;
}

/**
 * 上下文用量环：双圆 SVG 顺时针填充 + hover 提示。
 * 颜色随用量升高从 primary → warning → error。
 */
export function TokenIndicator({
    percentage,
    usedTokens,
    maxTokens,
    size = 14,
}: TokenIndicatorProps) {
    const { t } = useTranslation();
    const pct = Math.max(0, Math.min(100, percentage));
    const radius = (size - 3) / 2;
    const center = size / 2;
    const circumference = 2 * Math.PI * radius;
    const offset = circumference * (1 - pct / 100);

    const color = pct >= 90 ? 'text-error' : pct >= 70 ? 'text-warning' : 'text-primary';

    const usedText = formatTokens(usedTokens);
    const maxText = formatTokens(maxTokens);
    const tooltip =
        usedText && maxText
            ? `${pct.toFixed(1)}% · ${usedText} / ${maxText} ${t('chat.context')}`
            : t('chat.usagePercentage', { percentage: pct.toFixed(1) });

    return (
        <div className="group relative flex items-center gap-1" title={tooltip}>
            <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="-rotate-90">
                <circle
                    cx={center}
                    cy={center}
                    r={radius}
                    fill="none"
                    strokeWidth={2}
                    className="stroke-base-300"
                />
                <circle
                    cx={center}
                    cy={center}
                    r={radius}
                    fill="none"
                    strokeWidth={2}
                    strokeLinecap="round"
                    strokeDasharray={circumference}
                    strokeDashoffset={offset}
                    className={`${color} stroke-current transition-all`}
                />
            </svg>
            <span className="text-[11px] tabular-nums text-base-content/60">
                {Math.round(pct)}%
            </span>
        </div>
    );
}
