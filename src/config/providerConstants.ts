import type { AppType } from '../types/app';
import type { Provider } from '../types/provider';

export const CLAUDE_OFFICIAL_PROVIDER_ID = '__claude_official__';
export const CODEX_OFFICIAL_PROVIDER_ID = '__codex_official__';
export const OFFICIAL_PROVIDER_IDS = [
    CLAUDE_OFFICIAL_PROVIDER_ID,
    CODEX_OFFICIAL_PROVIDER_ID,
] as const;

const OFFICIAL_PROVIDER_ID_SET = new Set<string>(OFFICIAL_PROVIDER_IDS);
const OFFICIAL_PROVIDER_CREATED_AT = '1970-01-01T00:00:00.000Z';

export function isOfficialProvider(id: string) {
    return OFFICIAL_PROVIDER_ID_SET.has(id);
}

function supportsOfficialProvider(app: AppType) {
    return app === 'claude' || app === 'codex';
}

function buildOfficialProvider(app: Extract<AppType, 'claude' | 'codex'>, isActive: boolean): Provider {
    return {
        id: app === 'claude' ? CLAUDE_OFFICIAL_PROVIDER_ID : CODEX_OFFICIAL_PROVIDER_ID,
        name: app === 'claude' ? 'Claude 官方订阅' : 'Codex 官方订阅',
        appType: app,
        apiKey: '',
        url: undefined,
        defaultSonnetModel: undefined,
        defaultOpusModel: undefined,
        defaultHaikuModel: undefined,
        defaultReasoningModel: undefined,
        customParams: undefined,
        settingsConfig: undefined,
        meta: { official: 'true' },
        icon: 'official',
        inFailoverQueue: false,
        description: undefined,
        tags: [],
        isActive,
        createdAt: OFFICIAL_PROVIDER_CREATED_AT,
        lastUsed: undefined,
        proxyConfig: undefined,
        oneMContext: undefined,
    };
}

function officialAppsForScope(app?: AppType): Array<Extract<AppType, 'claude' | 'codex'>> {
    if (app === 'claude' || app === 'codex') return [app];
    if (app && !supportsOfficialProvider(app)) return [];
    return ['claude', 'codex'];
}

export function mergeOfficialProviders(providers: Provider[], app?: AppType) {
    const customProviders = providers.filter(provider => !isOfficialProvider(provider.id));
    const officialProviders = officialAppsForScope(app).map((officialApp) => {
        // 中文注释：官方 Provider 不在 DB 中；若该 app 没有任何自定义 Provider 处于 active，
        // 则推断当前应回落官方订阅登录态，并把合成项标记为 active。
        const hasActiveCustomProvider = customProviders.some(
            provider => provider.appType === officialApp && provider.isActive
        );
        return buildOfficialProvider(officialApp, !hasActiveCustomProvider);
    });

    return [...officialProviders, ...customProviders];
}
