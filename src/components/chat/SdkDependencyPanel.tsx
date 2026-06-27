import {useEffect, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {Check, Cpu, Download, Loader2, Package, RefreshCw, RotateCw, Trash2} from 'lucide-react';
import {useSdkStore} from '../../stores/useSdkStore';
import type {SdkStatus} from '../../types/chat';

const SDK_DEPENDENCY_PANEL_FALLBACKS = {
    title: 'SDK Dependencies',
    close: 'Close',
    cancel: 'Cancel',
    refresh: 'Refresh',
    hint: 'SDKs are not bundled. First-time use requires installing them locally via system npm.',
    installed: 'Installed',
    notInstalled: 'Not installed',
    uninstall: 'Uninstall',
    installing: 'Installing...',
    install: 'Install',
    installLog: 'Install log',
    targetVersion: 'Target version',
    descriptionClaude: 'Required for Claude AI features. Includes Claude Code SDK and related dependencies.',
    descriptionCodex: 'Required for Codex AI features. Includes OpenAI Codex SDK.',
    descriptionGeneric: 'Required for {{name}} features.',
    currentVersion: 'Current {{version}}',
    latestStableVersion: 'Latest stable {{version}}',
    defaultVersion: 'Default {{version}}',
    updateAvailable: 'Update available',
    currentVersionAction: 'Current version',
    installVersion: 'Install {{version}}',
    updateToVersion: 'Update to {{version}}',
    switchToVersion: 'Switch to {{version}}',
    noVersions: 'No versions available',
    nodeRuntimeTitle: 'Node.js runtime',
    nodeRuntimeMissing: 'Node.js is required before installing SDKs.',
    nodeRuntimePrivateInstall: 'Install private runtime',
    nodeRuntimeInstalling: 'Installing runtime...',
    nodeRuntimeNoSystemChange: 'Uses CCG Switch private data only and does not modify system PATH.',
};

type TranslateOptions = Record<string, string>;
type TranslateFn = (key: string, options?: TranslateOptions) => string;

function interpolate(template: string, options?: TranslateOptions): string {
    return Object.entries(options ?? {}).reduce(
        (text, [name, value]) => text.split(`{{${name}}}`).join(value),
        template,
    );
}

function translateWithFallback(
    t: TranslateFn,
    key: string,
    fallback: string,
    options?: TranslateOptions,
): string {
    const translated = t(key, options);
    return interpolate(translated === key ? fallback : translated, options);
}

function formatVersion(version: string): string {
    return version.startsWith('v') ? version : `v${version}`;
}

export function getSdkDependencyPanelLabels(t: TranslateFn) {
    return {
        title: translateWithFallback(t, 'chat.sdk.title', SDK_DEPENDENCY_PANEL_FALLBACKS.title),
        close: translateWithFallback(t, 'common.close', SDK_DEPENDENCY_PANEL_FALLBACKS.close),
        cancel: translateWithFallback(t, 'common.cancel', SDK_DEPENDENCY_PANEL_FALLBACKS.cancel),
        refresh: translateWithFallback(t, 'chat.sdk.refresh', SDK_DEPENDENCY_PANEL_FALLBACKS.refresh),
        hint: translateWithFallback(t, 'chat.sdk.hint', SDK_DEPENDENCY_PANEL_FALLBACKS.hint),
        installed: translateWithFallback(t, 'chat.sdk.installed', SDK_DEPENDENCY_PANEL_FALLBACKS.installed),
        notInstalled: translateWithFallback(t, 'chat.sdk.notInstalled', SDK_DEPENDENCY_PANEL_FALLBACKS.notInstalled),
        uninstall: translateWithFallback(t, 'chat.sdk.uninstall', SDK_DEPENDENCY_PANEL_FALLBACKS.uninstall),
        installing: translateWithFallback(t, 'chat.sdk.installing', SDK_DEPENDENCY_PANEL_FALLBACKS.installing),
        install: translateWithFallback(t, 'chat.sdk.install', SDK_DEPENDENCY_PANEL_FALLBACKS.install),
        installLog: translateWithFallback(t, 'chat.sdk.installLog', SDK_DEPENDENCY_PANEL_FALLBACKS.installLog),
        targetVersion: translateWithFallback(t, 'chat.sdk.targetVersion', SDK_DEPENDENCY_PANEL_FALLBACKS.targetVersion),
        description: (sdk: SdkStatus) => {
            const sdkIdentity = `${sdk.id} ${sdk.displayName}`.toLowerCase();
            if (sdkIdentity.includes('claude')) {
                return translateWithFallback(
                    t,
                    'chat.sdk.description.claude',
                    SDK_DEPENDENCY_PANEL_FALLBACKS.descriptionClaude,
                );
            }
            if (sdkIdentity.includes('codex')) {
                return translateWithFallback(
                    t,
                    'chat.sdk.description.codex',
                    SDK_DEPENDENCY_PANEL_FALLBACKS.descriptionCodex,
                );
            }
            return translateWithFallback(
                t,
                'chat.sdk.description.generic',
                SDK_DEPENDENCY_PANEL_FALLBACKS.descriptionGeneric,
                {name: sdk.displayName},
            );
        },
        currentVersion: (version: string) => translateWithFallback(
            t,
            'chat.sdk.currentVersion',
            SDK_DEPENDENCY_PANEL_FALLBACKS.currentVersion,
            {version: formatVersion(version)},
        ),
        latestStableVersion: (version: string) => translateWithFallback(
            t,
            'chat.sdk.latestStableVersion',
            SDK_DEPENDENCY_PANEL_FALLBACKS.latestStableVersion,
            {version: formatVersion(version)},
        ),
        defaultVersion: (version: string) => translateWithFallback(
            t,
            'chat.sdk.defaultVersion',
            SDK_DEPENDENCY_PANEL_FALLBACKS.defaultVersion,
            {version},
        ),
        updateAvailable: translateWithFallback(t, 'chat.sdk.updateAvailable', SDK_DEPENDENCY_PANEL_FALLBACKS.updateAvailable),
        currentVersionAction: translateWithFallback(t, 'chat.sdk.currentVersionAction', SDK_DEPENDENCY_PANEL_FALLBACKS.currentVersionAction),
        installVersion: (version: string) => translateWithFallback(
            t,
            'chat.sdk.installVersion',
            SDK_DEPENDENCY_PANEL_FALLBACKS.installVersion,
            {version: formatVersion(version)},
        ),
        updateToVersion: (version: string) => translateWithFallback(
            t,
            'chat.sdk.updateToVersion',
            SDK_DEPENDENCY_PANEL_FALLBACKS.updateToVersion,
            {version: formatVersion(version)},
        ),
        switchToVersion: (version: string) => translateWithFallback(
            t,
            'chat.sdk.switchToVersion',
            SDK_DEPENDENCY_PANEL_FALLBACKS.switchToVersion,
            {version: formatVersion(version)},
        ),
        noVersions: translateWithFallback(t, 'chat.sdk.noVersions', SDK_DEPENDENCY_PANEL_FALLBACKS.noVersions),
        nodeRuntimeTitle: translateWithFallback(t, 'chat.sdk.nodeRuntime.title', SDK_DEPENDENCY_PANEL_FALLBACKS.nodeRuntimeTitle),
        nodeRuntimeMissing: translateWithFallback(t, 'chat.sdk.nodeRuntime.missing', SDK_DEPENDENCY_PANEL_FALLBACKS.nodeRuntimeMissing),
        nodeRuntimePrivateInstall: translateWithFallback(
            t,
            'chat.sdk.nodeRuntime.privateInstall',
            SDK_DEPENDENCY_PANEL_FALLBACKS.nodeRuntimePrivateInstall,
        ),
        nodeRuntimeInstalling: translateWithFallback(t, 'chat.sdk.nodeRuntime.installing', SDK_DEPENDENCY_PANEL_FALLBACKS.nodeRuntimeInstalling),
        nodeRuntimeNoSystemChange: translateWithFallback(
            t,
            'chat.sdk.nodeRuntime.noSystemChange',
            SDK_DEPENDENCY_PANEL_FALLBACKS.nodeRuntimeNoSystemChange,
        ),
    };
}

type SdkDependencyPanelLabels = ReturnType<typeof getSdkDependencyPanelLabels>;
type SdkVersionActionKind = 'install' | 'update' | 'current' | 'switch' | 'unavailable';

interface SdkVersionActionArgs {
    installed: boolean;
    currentVersion?: string | null;
    targetVersion?: string;
    labels: SdkDependencyPanelLabels;
}

export interface SdkVersionAction {
    kind: SdkVersionActionKind;
    label: string;
    disabled: boolean;
}

function compareExactVersions(left?: string | null, right?: string | null): number | null {
    if (!left || !right) return null;
    const leftParts = left.split('.').map((part) => Number.parseInt(part, 10));
    const rightParts = right.split('.').map((part) => Number.parseInt(part, 10));
    if (
        leftParts.length !== 3
        || rightParts.length !== 3
        || leftParts.some(Number.isNaN)
        || rightParts.some(Number.isNaN)
    ) {
        return null;
    }

    for (let index = 0; index < 3; index += 1) {
        if (leftParts[index] > rightParts[index]) return 1;
        if (leftParts[index] < rightParts[index]) return -1;
    }
    return 0;
}

export function getSdkDependencyVersionAction({
    installed,
    currentVersion,
    targetVersion,
    labels,
}: SdkVersionActionArgs): SdkVersionAction {
    if (!targetVersion) {
        return {
            kind: 'unavailable',
            label: labels.noVersions,
            disabled: true,
        };
    }

    if (!installed) {
        return {
            kind: 'install',
            label: labels.installVersion(targetVersion),
            disabled: false,
        };
    }

    if (currentVersion && targetVersion === currentVersion) {
        return {
            kind: 'current',
            label: labels.currentVersionAction,
            disabled: true,
        };
    }

    const comparison = compareExactVersions(targetVersion, currentVersion);
    if (comparison !== null && comparison > 0) {
        return {
            kind: 'update',
            label: labels.updateToVersion(targetVersion),
            disabled: false,
        };
    }

    return {
        kind: 'switch',
        label: labels.switchToVersion(targetVersion),
        disabled: false,
    };
}

function getVersionOptions(sdk: SdkStatus): string[] {
    const versions = [
        ...sdk.availableVersions,
        sdk.latestVersion ?? '',
        sdk.currentVersion ?? '',
    ].filter(Boolean);
    return Array.from(new Set(versions));
}

function getPreferredTargetVersion(sdk: SdkStatus): string {
    const options = getVersionOptions(sdk);
    const preferred = sdk.latestVersion ?? sdk.currentVersion ?? options[0];
    return preferred && options.includes(preferred) ? preferred : options[0] ?? '';
}

function getEffectiveTargetVersion(sdk: SdkStatus, selectedVersion?: string): string {
    const options = getVersionOptions(sdk);
    if (selectedVersion && options.includes(selectedVersion)) {
        return selectedVersion;
    }
    return getPreferredTargetVersion(sdk);
}

function getPrimaryActionClass(action: SdkVersionAction, installed: boolean): string {
    const baseClass = 'btn btn-sm h-9 min-h-9 justify-center gap-2 border-none px-4 shadow-none';
    if (action.kind === 'current') {
        return `${baseClass} sdk-action-current bg-blue-50 text-blue-700 disabled:bg-blue-50 disabled:text-blue-700 disabled:opacity-70 dark:bg-info/10 dark:text-info dark:disabled:bg-info/10 dark:disabled:text-info`;
    }
    if (action.kind === 'install' && !installed) {
        return `${baseClass} bg-blue-50 text-blue-700 hover:bg-blue-100 dark:bg-info/15 dark:text-info dark:hover:bg-info/20`;
    }
    if (action.kind === 'unavailable') {
        return `${baseClass} bg-slate-100 text-slate-500 disabled:bg-slate-100 disabled:text-slate-500 dark:bg-base-300/60 dark:text-base-content/50`;
    }
    return `${baseClass} bg-blue-50 text-blue-700 hover:bg-blue-100 dark:bg-info/15 dark:text-info dark:hover:bg-info/20`;
}

/**
 * SDK 依赖管理面板 —— 安装 / 卸载 Claude / Codex SDK。
 *
 * SDK 不随应用打包，首次使用需通过本面板用系统 npm 安装到
 * ~/.ccg-switch/ai-bridge-deps/<sdkId>/。
 */
export default function SdkDependencyPanel() {
    const { t } = useTranslation();
    const {
        statuses,
        installing,
        logs,
        error,
        nodeRuntimeStatus,
        nodeRuntimeInstalling,
        nodeRuntimeLogs,
        init,
        install,
        uninstall,
        installNodeRuntime,
        refresh,
    } =
        useSdkStore();
    const [selectedVersions, setSelectedVersions] = useState<Record<string, string>>({});
    const labels = getSdkDependencyPanelLabels(t);
    const nodeRuntimeMissing = nodeRuntimeStatus?.installed === false;
    const showNodeRuntimeCard = nodeRuntimeInstalling || nodeRuntimeMissing;
    const sdkInstallBlocked = nodeRuntimeMissing || nodeRuntimeInstalling;
    const activeLogs = (nodeRuntimeInstalling || nodeRuntimeLogs.length > 0) ? nodeRuntimeLogs : logs;
    const activeLogPlaceholder = nodeRuntimeInstalling ? labels.nodeRuntimeInstalling : labels.installing;

    useEffect(() => {
        init();
    }, [init]);

    return (
        <div className="space-y-3">
            <div className="flex justify-end">
                <button
                    type="button"
                    className="btn btn-ghost btn-xs h-7 min-h-7 gap-1.5 px-2 text-slate-600 hover:bg-slate-100 dark:text-base-content/70 dark:hover:bg-base-200/70"
                    title={labels.refresh}
                    aria-label={labels.refresh}
                    onClick={refresh}
                >
                    <RefreshCw size={14} />
                    {labels.refresh}
                </button>
            </div>

            {showNodeRuntimeCard && (
                <div className="sdk-node-runtime-card rounded-lg border border-amber-200 bg-amber-50/70 p-4 shadow-sm dark:border-warning/25 dark:bg-warning/10 dark:shadow-none">
                    <div className="flex flex-wrap items-center gap-2">
                        <span className="inline-flex h-4 w-4 shrink-0 items-center justify-center text-amber-700 dark:text-warning">
                            {nodeRuntimeInstalling ? (
                                <Loader2 size={15} className="animate-spin" />
                            ) : (
                                <Cpu size={15} />
                            )}
                        </span>
                        <span className="min-w-0 truncate text-base font-semibold leading-5 text-slate-900 dark:text-base-content">
                            {labels.nodeRuntimeTitle}
                        </span>
                        <span className="rounded border border-amber-200 bg-white/70 px-2 py-0.5 text-[11px] font-semibold leading-4 text-amber-800 dark:border-transparent dark:bg-warning/15 dark:text-warning">
                            {nodeRuntimeStatus?.version ?? 'v24.11.1'}
                        </span>
                    </div>
                    <p className="mt-1 text-sm leading-5 text-slate-700 dark:text-base-content/70">
                        {labels.nodeRuntimeMissing}
                    </p>
                    <p className="mt-1 text-xs leading-5 text-slate-600 dark:text-base-content/55">
                        {labels.nodeRuntimeNoSystemChange}
                    </p>
                    {nodeRuntimeStatus?.installDir && (
                        <p className="mt-1 truncate text-xs leading-5 text-slate-500 dark:text-base-content/45" title={nodeRuntimeStatus.installDir}>
                            {nodeRuntimeStatus.installDir}
                        </p>
                    )}
                    <button
                        type="button"
                        className="btn btn-sm mt-3 h-9 min-h-9 justify-center gap-2 border-none bg-amber-100 px-4 text-amber-800 shadow-none hover:bg-amber-200 disabled:bg-amber-100 disabled:text-amber-800 disabled:opacity-60 dark:bg-warning/15 dark:text-warning dark:hover:bg-warning/20 dark:disabled:bg-warning/15 dark:disabled:text-warning"
                        title={labels.nodeRuntimePrivateInstall}
                        aria-label={labels.nodeRuntimePrivateInstall}
                        onClick={installNodeRuntime}
                        disabled={nodeRuntimeInstalling}
                    >
                        {nodeRuntimeInstalling ? (
                            <Loader2 size={15} className="animate-spin" />
                        ) : (
                            <Download size={15} />
                        )}
                        <span>{nodeRuntimeInstalling ? labels.nodeRuntimeInstalling : labels.nodeRuntimePrivateInstall}</span>
                    </button>
                </div>
            )}

            <div className="sdk-dependency-list space-y-4">
                {statuses.map((sdk) => {
                    const isInstalling = installing === sdk.id;
                    const versionOptions = getVersionOptions(sdk);
                    const targetVersion = getEffectiveTargetVersion(sdk, selectedVersions[sdk.id]);
                    const action = getSdkDependencyVersionAction({
                        installed: sdk.installed,
                        currentVersion: sdk.currentVersion,
                        targetVersion,
                        labels,
                    });
                    const actionDisabled = !!installing || sdkInstallBlocked || action.disabled;
                    const primaryActionLabel = isInstalling ? labels.installing : action.label;
                    const currentChipText = sdk.currentVersion
                        ? formatVersion(sdk.currentVersion)
                        : (sdk.installed ? labels.installed : labels.notInstalled);
                    const latestChipText = sdk.latestVersion ? `→ ${formatVersion(sdk.latestVersion)}` : null;
                    const hasUpdate = !!(
                        sdk.installed
                        && sdk.latestVersion
                        && sdk.currentVersion
                        && sdk.latestVersion !== sdk.currentVersion
                    );
                    const metaItems = [
                        sdk.currentVersion ? labels.currentVersion(sdk.currentVersion) : null,
                        sdk.latestVersion ? labels.latestStableVersion(sdk.latestVersion) : null,
                    ].filter((item): item is string => Boolean(item));

                    return (
                        <div
                            key={sdk.id}
                            className="sdk-dependency-card rounded-lg border border-slate-200 bg-white p-4 shadow-sm dark:border-base-300 dark:bg-base-200/40 dark:shadow-none"
                        >
                            <div className="sdk-card-header flex flex-wrap items-center gap-2">
                                <span
                                    className="inline-flex h-4 w-4 shrink-0 items-center justify-center text-slate-600 dark:text-base-content/80"
                                    title={`${sdk.installed ? labels.installed : labels.notInstalled}: ${sdk.displayName}`}
                                    aria-label={`${sdk.installed ? labels.installed : labels.notInstalled}: ${sdk.displayName}`}
                                >
                                    {sdk.installed ? (
                                        <Check size={14} />
                                    ) : (
                                        <Package size={14} />
                                    )}
                                </span>
                                <span className="min-w-0 truncate text-base font-semibold leading-5 text-slate-900 dark:text-base-content">
                                    {sdk.displayName}
                                </span>
                                <span className="sdk-version-chip rounded border border-slate-200 bg-slate-100 px-2 py-0.5 text-[11px] font-semibold leading-4 text-slate-700 dark:border-transparent dark:bg-base-300/80 dark:text-base-content">
                                    {currentChipText}
                                </span>
                                {latestChipText && (
                                    <span className="sdk-version-arrow-chip rounded border border-slate-200 bg-slate-100 px-2 py-0.5 text-[11px] font-semibold leading-4 text-slate-700 dark:border-transparent dark:bg-base-300/80 dark:text-base-content">
                                        {latestChipText}
                                    </span>
                                )}
                                {hasUpdate && (
                                    <span className="sdk-update-pill rounded border border-blue-100 bg-blue-50 px-2 py-0.5 text-[11px] font-medium leading-4 text-blue-700 dark:border-transparent dark:bg-info/15 dark:text-info">
                                        {labels.updateAvailable}
                                    </span>
                                )}
                            </div>

                            <p className="mt-1 text-sm leading-5 text-slate-600 dark:text-base-content/55">
                                {labels.description(sdk)}
                            </p>

                            <label className="sdk-target-select-row mt-4 grid grid-cols-[auto,minmax(0,1fr)] items-center gap-3">
                                <span className="text-xs font-semibold text-slate-700 dark:text-base-content">
                                    {labels.targetVersion}
                                </span>
                                <select
                                    className="select select-bordered select-sm h-8 min-h-8 w-full rounded-md border-slate-200 bg-white text-sm text-slate-900 shadow-none dark:border-base-300 dark:bg-base-100/60 dark:text-base-content"
                                    value={targetVersion}
                                    disabled={!!installing || sdkInstallBlocked || versionOptions.length === 0}
                                    aria-label={`${labels.targetVersion}: ${sdk.displayName}`}
                                    onChange={(event) => {
                                        setSelectedVersions((prev) => ({
                                            ...prev,
                                            [sdk.id]: event.target.value,
                                        }));
                                    }}
                                >
                                    {versionOptions.length === 0 ? (
                                        <option value="">{labels.noVersions}</option>
                                    ) : (
                                        versionOptions.map((version) => (
                                            <option key={version} value={version}>
                                                {formatVersion(version)}
                                            </option>
                                        ))
                                    )}
                                </select>
                            </label>

                            <div className="sdk-card-actions mt-3 flex flex-wrap items-center gap-2">
                                <button
                                    type="button"
                                    className={getPrimaryActionClass(action, sdk.installed)}
                                    title={`${primaryActionLabel}: ${sdk.displayName}`}
                                    aria-label={`${primaryActionLabel}: ${sdk.displayName}`}
                                    onClick={() => install(sdk.id, targetVersion || undefined)}
                                    disabled={actionDisabled}
                                >
                                    {isInstalling ? (
                                        <Loader2 size={15} className="animate-spin" />
                                    ) : action.kind === 'install' ? (
                                        <Download size={15} />
                                    ) : (
                                        <RotateCw size={15} />
                                    )}
                                    <span>{primaryActionLabel}</span>
                                </button>
                                {sdk.installed && (
                                    <button
                                        type="button"
                                        className="btn btn-sm h-9 min-h-9 gap-2 border-none bg-red-50 px-4 text-red-700 hover:bg-red-100 shadow-none disabled:bg-red-50 disabled:text-red-700 disabled:opacity-60 dark:bg-error/10 dark:text-error dark:hover:bg-error/15 dark:disabled:bg-error/10 dark:disabled:text-error"
                                        title={labels.uninstall}
                                        aria-label={`${labels.uninstall}: ${sdk.displayName}`}
                                        onClick={() => uninstall(sdk.id)}
                                        disabled={!!installing || sdkInstallBlocked}
                                    >
                                        <Trash2 size={15} />
                                        {labels.uninstall}
                                    </button>
                                )}
                            </div>

                            <div className="sdk-card-meta mt-2 flex flex-wrap gap-x-3 gap-y-1 text-xs text-slate-500 dark:text-base-content/45">
                                {metaItems.map((item) => (
                                    <span key={item}>{item}</span>
                                ))}
                                {sdk.installed && sdk.path && (
                                    <span className="max-w-full truncate" title={sdk.path}>
                                        {sdk.path}
                                    </span>
                                )}
                            </div>
                        </div>
                    );
                })}
            </div>

            {error && <div className="alert alert-error py-2 text-sm">{error}</div>}

            {(installing || nodeRuntimeInstalling || activeLogs.length > 0) && (
                <details
                    open={!!installing || nodeRuntimeInstalling}
                    className="sdk-install-log rounded-lg border border-slate-200 bg-white text-xs text-slate-600 shadow-sm dark:border-base-300 dark:bg-base-200/60 dark:text-base-content/70 dark:shadow-none"
                >
                    <summary className="cursor-pointer select-none px-3 py-2 text-xs font-semibold text-slate-700 marker:text-slate-400 dark:text-base-content dark:marker:text-base-content/45">
                        {labels.installLog}
                    </summary>
                    <div className="max-h-28 overflow-y-auto border-t border-slate-100 px-3 py-2 font-mono leading-5 text-slate-500 dark:border-base-300/60 dark:text-base-content/60">
                        {activeLogs.length === 0 ? (
                            <div className="whitespace-pre-wrap">{activeLogPlaceholder}</div>
                        ) : (
                            activeLogs.map((line, i) => (
                                <div key={i} className="whitespace-pre-wrap">
                                    {line}
                                </div>
                            ))
                        )}
                    </div>
                </details>
            )}
        </div>
    );
}
