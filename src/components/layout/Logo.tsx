import appIcon from '../../assets/app-icon.png';

interface LogoProps {
    size?: 'sm' | 'md';
}

export default function Logo({ size = 'md' }: LogoProps) {
    const iconSize = size === 'sm' ? 24 : 28;
    const textSize = size === 'sm' ? 'text-sm' : 'text-base';

    return (
        <div className="flex items-center gap-2.5 select-none">
            <img
                src={appIcon}
                alt="JadeKit"
                width={iconSize}
                height={iconSize}
                className="shrink-0 rounded-md"
            />
            <span className={`${textSize} font-bold bg-gradient-to-r from-slate-700 via-slate-500 to-cyan-500 bg-clip-text text-transparent dark:from-slate-100 dark:via-slate-300 dark:to-cyan-200`}>
                JadeKit
            </span>
        </div>
    );
}
