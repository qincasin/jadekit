import type { ToolStatus } from '../../types/toolblock';

interface StatusIndicatorProps {
    status: ToolStatus;
}

/**
 * 工具状态指示器 - 显示小圆点表示工具调用状态
 */
export default function StatusIndicator({ status }: StatusIndicatorProps) {
    return (
        <div
            className={`
                w-2 h-2 rounded-full transition-all duration-300
                ${status === 'pending' ? 'bg-warning animate-pulse shadow-lg shadow-warning/50' : ''}
                ${status === 'completed' ? 'bg-success shadow-sm shadow-success/30' : ''}
                ${status === 'error' ? 'bg-error animate-pulse shadow-lg shadow-error/50' : ''}
            `}
            title={
                status === 'pending' ? '执行中...' :
                status === 'completed' ? '已完成' :
                '执行失败'
            }
        />
    );
}
