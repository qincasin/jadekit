interface BouncingDotsProps {
    size?: number;
    className?: string;
}

/**
 * 跳动的三个点 loading 动画
 * 三个点依次从下到上跳动，循环播放
 */
export default function BouncingDots({ size = 4, className = '' }: BouncingDotsProps) {
    return (
        <div className={`inline-flex items-center gap-1 ${className}`} aria-hidden="true">
            <span
                className="rounded-full bg-current animate-bounce"
                style={{
                    width: size,
                    height: size,
                    animationDelay: '0ms',
                    animationDuration: '1.4s',
                }}
            />
            <span
                className="rounded-full bg-current animate-bounce"
                style={{
                    width: size,
                    height: size,
                    animationDelay: '160ms',
                    animationDuration: '1.4s',
                }}
            />
            <span
                className="rounded-full bg-current animate-bounce"
                style={{
                    width: size,
                    height: size,
                    animationDelay: '320ms',
                    animationDuration: '1.4s',
                }}
            />
        </div>
    );
}
