import { Link, useLocation } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Sun, Moon } from 'lucide-react';
import { useConfigStore } from '../../stores/useConfigStore';
import Logo from './Logo';
import {
    LayoutDashboard, Key, Globe, FileText, Zap,
    Bot, FolderOpen, Settings, Server, Rocket, MessageSquare
} from 'lucide-react';

interface SidebarProps {
    position: 'left' | 'right' | 'top';
}

const mainNavItems = [
    { path: '/', icon: LayoutDashboard, labelKey: 'nav.dashboard' },
    { path: '/chat', icon: MessageSquare, labelKey: 'nav.chat' },
    { path: '/providers', icon: Key, labelKey: 'nav.providers' },
    { path: '/antigravity', icon: Rocket, labelKey: 'nav.antigravity' },
    { path: '/proxy', icon: Server, labelKey: 'nav.proxy' },
    { path: '/mcp', icon: Globe, labelKey: 'nav.mcp' },
    { path: '/prompts', icon: FileText, labelKey: 'nav.prompts' },
    { path: '/skills', icon: Zap, labelKey: 'nav.skills' },
    { path: '/subagents', icon: Bot, labelKey: 'nav.subagents' },
    { path: '/workspaces', icon: FolderOpen, labelKey: 'nav.workspaces' },
];

const bottomNavItems = [
    { path: '/settings', icon: Settings, labelKey: 'nav.settings' },
];

function NavTooltip({ children, position }: { children: string; position: 'bottom' | 'left' | 'right' }) {
    const positionClass = {
        bottom: 'left-1/2 top-full mt-2 -translate-x-1/2',
        left: 'right-full mr-2 top-1/2 -translate-y-1/2',
        right: 'left-full ml-2 top-1/2 -translate-y-1/2',
    }[position];

    return (
        <span
            className={`
                pointer-events-none absolute ${positionClass} z-[9999]
                whitespace-nowrap rounded-md bg-gray-900 px-2 py-1 text-xs font-medium text-white shadow-lg
                opacity-0 transition-opacity duration-150
                group-hover:opacity-100 group-focus-visible:opacity-100
                dark:bg-gray-100 dark:text-gray-900
            `}
        >
            {children}
        </span>
    );
}

export default function Sidebar({ position }: SidebarProps) {
    const location = useLocation();
    const { t, i18n } = useTranslation();
    const { config, saveConfig } = useConfigStore();

    const isActive = (path: string) => {
        if (path === '/') return location.pathname === '/';
        return location.pathname.startsWith(path);
    };

    const allItems = [...mainNavItems, ...bottomNavItems];

    // ── 顶部布局 ──
    if (position === 'top') {
        const toggleTheme = async () => {
            if (!config) return;
            const newTheme = config.theme === 'light' ? 'dark' : 'light';
            await saveConfig({ ...config, theme: newTheme });
        };

        const toggleLang = async () => {
            if (!config) return;
            const newLang = config.language === 'zh' ? 'en' : 'zh';
            await saveConfig({ ...config, language: newLang });
            i18n.changeLanguage(newLang);
        };

        return (
            <nav className="w-full pt-8">
                <div className="relative flex items-center justify-center px-6 h-14">
                    {/* 左侧 Logo */}
                    <div className="absolute left-6">
                        <Logo />
                    </div>
                    {/* 居中：导航图标 */}
                    <div className="flex items-center gap-1">
                        {allItems.map(item => {
                            const Icon = item.icon;
                            const active = isActive(item.path);
                            return (
                                <Link
                                    key={item.path}
                                    to={item.path}
                                    aria-label={t(item.labelKey)}
                                    className={`
                                            relative group
                                            flex items-center justify-center w-10 h-10 rounded-lg
                                            transition-all duration-200
                                            ${active
                                                ? 'bg-gray-200/80 dark:bg-base-300 text-orange-500 dark:text-orange-400 shadow-sm'
                                                : 'text-gray-400 dark:text-gray-500 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-base-200'
                                            }
                                        `}
                                >
                                    <Icon className="w-[22px] h-[22px]" strokeWidth={active ? 2 : 1.6} />
                                    <NavTooltip position="bottom">{t(item.labelKey)}</NavTooltip>
                                </Link>
                            );
                        })}
                    </div>

                    {/* 右侧绝对定位：主题 + 语言 */}
                    <div className="absolute right-6 flex items-center gap-1.5">
                        <button
                            onClick={toggleTheme}
                            className="w-9 h-9 rounded-lg bg-gray-100 dark:bg-base-200 hover:bg-gray-200 dark:hover:bg-base-300 flex items-center justify-center transition-colors"
                        >
                            {config?.theme === 'light'
                                ? <Moon className="w-4 h-4 text-gray-600 dark:text-gray-300" />
                                : <Sun className="w-4 h-4 text-gray-600 dark:text-gray-300" />
                            }
                        </button>
                        <button
                            onClick={toggleLang}
                            className="w-9 h-9 rounded-lg bg-gray-100 dark:bg-base-200 hover:bg-gray-200 dark:hover:bg-base-300 flex items-center justify-center transition-colors"
                        >
                            <span className="text-xs font-bold text-gray-600 dark:text-gray-300">
                                {config?.language === 'zh' ? 'EN' : '中'}
                            </span>
                        </button>
                    </div>
                </div>
            </nav>
        );
    }

    // ── 左/右 竖向布局 ──
    const renderNavItem = (item: typeof mainNavItems[0]) => {
        const Icon = item.icon;
        const active = isActive(item.path);
        const tooltipPosition = position === 'left' ? 'right' : 'left';
        const indicatorClass = position === 'left'
            ? 'left-0 top-1/2 -translate-y-1/2 w-[3px] h-6 rounded-r-full'
            : 'right-0 top-1/2 -translate-y-1/2 w-[3px] h-6 rounded-l-full';

        return (
            <Link
                key={item.path}
                to={item.path}
                aria-label={t(item.labelKey)}
                className={`
                        relative flex items-center justify-center w-12 h-12 rounded-xl
                        transition-all duration-200 group
                        ${active
                            ? 'bg-orange-50 dark:bg-orange-500/10 text-orange-600 dark:text-orange-400'
                            : 'text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-base-200 hover:text-gray-700 dark:hover:text-gray-200'
                        }
                    `}
            >
                {active && (
                    <div className={`absolute ${indicatorClass} bg-orange-500`} />
                )}
                <Icon className="w-5 h-5" strokeWidth={active ? 2.2 : 1.8} />
                <NavTooltip position={tooltipPosition}>{t(item.labelKey)}</NavTooltip>
            </Link>
        );
    };

    const borderClass = position === 'left'
        ? 'border-r border-gray-200 dark:border-base-200'
        : 'border-l border-gray-200 dark:border-base-200';

    return (
        <aside className={`w-16 h-full flex flex-col items-center bg-white dark:bg-base-100 ${borderClass} pt-12 pb-4`}>
            <nav className="flex-1 flex flex-col items-center gap-1 mt-2">
                {mainNavItems.map(renderNavItem)}
            </nav>
            <nav className="flex flex-col items-center gap-1">
                {bottomNavItems.map(renderNavItem)}
            </nav>
        </aside>
    );
}
