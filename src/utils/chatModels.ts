import type {Provider} from '../types/provider';
import {type ChatProviderId, CLAUDE_MODELS, CODEX_MODELS, type ModelInfo,} from '../components/chat/composer/constants';

export const CHAT_MODEL_SELECTION_KEY_PREFIX = 'ccg-chat-model:';
export const CHAT_CUSTOM_MODEL_STORAGE_PREFIX = 'ccg-chat-custom-models:';

interface ChatModelStorage {
    getItem: (key: string) => string | null;
    setItem?: (key: string, value: string) => void;
}

interface BuildChatModelListOptions {
    storage?: ChatModelStorage | null;
}

interface StoreFetchedChatModelsOptions {
    storage?: ChatModelStorage | null;
    eventTarget?: Pick<EventTarget, 'dispatchEvent'> | null;
}

export interface ChatModelRefreshSource {
    providerName: string;
    url: string;
    apiKey: string;
}

const PROVIDER_MODEL_FIELDS: Array<keyof Pick<
    Provider,
    'defaultSonnetModel' | 'defaultOpusModel' | 'defaultHaikuModel' | 'defaultReasoningModel'
>> = [
    'defaultSonnetModel',
    'defaultOpusModel',
    'defaultHaikuModel',
    'defaultReasoningModel',
];

function defaultStorage(): ChatModelStorage | null {
    if (typeof window === 'undefined') return null;
    try {
        return window.localStorage;
    } catch {
        return null;
    }
}

function normalizeModelId(value: unknown): string {
    return typeof value === 'string' ? value.trim() : '';
}

function customModelStorageKey(provider: ChatProviderId): string {
    return CHAT_CUSTOM_MODEL_STORAGE_PREFIX + provider;
}

function defaultModelStorageEventTarget(): Pick<EventTarget, 'dispatchEvent'> | null {
    if (typeof window === 'undefined') return null;
    return window;
}

function createModelStorageChangeEvent(key: string): Event {
    if (typeof CustomEvent === 'function') {
        return new CustomEvent('localStorageChange', {detail: {key}});
    }

    const event = typeof Event === 'function'
        ? new Event('localStorageChange')
        : ({type: 'localStorageChange'} as Event);
    Object.defineProperty(event, 'detail', {
        value: {key},
        configurable: true,
    });
    return event;
}

function modelFromId(id: string, description?: string): ModelInfo {
    return {
        id,
        label: id,
        description,
    };
}

function mergeModelLists(lists: ModelInfo[][]): ModelInfo[] {
    const seen = new Set<string>();
    const fallbackById = new Map([
        ...CLAUDE_MODELS.map((model) => [model.id, model] as const),
        ...CODEX_MODELS.map((model) => [model.id, model] as const),
    ]);
    const merged: ModelInfo[] = [];

    lists.flat().forEach((model) => {
        const id = model.id.trim();
        if (!id || seen.has(id)) return;
        seen.add(id);

        const fallback = fallbackById.get(id);
        if (fallback && (!model.label || model.label === id)) {
            merged.push({
                ...fallback,
                description: model.description ?? fallback.description,
            });
            return;
        }

        merged.push({
            ...model,
            id,
            label: model.label?.trim() || id,
        });
    });

    return merged;
}

export function getFallbackChatModels(provider: ChatProviderId): ModelInfo[] {
    return provider === 'codex' ? CODEX_MODELS : CLAUDE_MODELS;
}

export function getDefaultChatModelId(provider: ChatProviderId): string {
    return getFallbackChatModels(provider)[0]?.id ?? '';
}

export function loadSavedChatModel(provider: ChatProviderId): string | null {
    const storage = defaultStorage();
    if (!storage) return null;
    try {
        const saved = storage.getItem(CHAT_MODEL_SELECTION_KEY_PREFIX + provider);
        return normalizeModelId(saved) || null;
    } catch {
        return null;
    }
}

export function parseStoredChatModels(raw: string | null | undefined): ModelInfo[] {
    if (!raw) return [];
    try {
        const parsed = JSON.parse(raw) as unknown;
        if (!Array.isArray(parsed)) return [];

        return mergeModelLists([
            parsed.flatMap((entry): ModelInfo[] => {
                if (typeof entry === 'string') {
                    const id = normalizeModelId(entry);
                    return id ? [modelFromId(id)] : [];
                }

                if (!entry || typeof entry !== 'object') return [];
                const record = entry as Record<string, unknown>;
                const id = normalizeModelId(record.id);
                if (!id) return [];
                const label = normalizeModelId(record.label) || id;
                const description = normalizeModelId(record.description) || undefined;
                return [{id, label, description}];
            }),
        ]);
    } catch {
        return [];
    }
}

export function loadStoredChatModels(
    provider: ChatProviderId,
    storage: ChatModelStorage | null = defaultStorage(),
): ModelInfo[] {
    if (!storage) return [];
    try {
        return parseStoredChatModels(storage.getItem(customModelStorageKey(provider)));
    } catch {
        return [];
    }
}

export function normalizeFetchedChatModelIds(modelIds: string[]): string[] {
    const seen = new Set<string>();
    return modelIds.flatMap((modelId) => {
        const id = normalizeModelId(modelId);
        if (!id || seen.has(id)) return [];
        seen.add(id);
        return [id];
    });
}

export function getChatModelRefreshSource(
    provider: ChatProviderId,
    providers: Provider[] = [],
): ChatModelRefreshSource | null {
    const matchingProviders = providers.filter((item) => item.appType === provider);
    const sourceProvider = matchingProviders.find((item) => item.isActive) ?? matchingProviders[0];
    if (!sourceProvider) return null;

    const url = normalizeModelId(sourceProvider.url);
    const apiKey = normalizeModelId(sourceProvider.apiKey);
    if (!url || !apiKey) return null;

    return {
        providerName: sourceProvider.name,
        url,
        apiKey,
    };
}

export function storeFetchedChatModels(
    provider: ChatProviderId,
    modelIds: string[],
    options: StoreFetchedChatModelsOptions = {},
): number {
    const storage = options.storage === undefined ? defaultStorage() : options.storage;
    if (!storage?.setItem) return 0;

    const ids = normalizeFetchedChatModelIds(modelIds);
    const key = customModelStorageKey(provider);
    storage.setItem(key, JSON.stringify(ids));

    const eventTarget = options.eventTarget === undefined ? defaultModelStorageEventTarget() : options.eventTarget;
    eventTarget?.dispatchEvent(createModelStorageChangeEvent(key));

    return ids.length;
}

function providerModelsFor(provider: ChatProviderId, providers: Provider[]): ModelInfo[] {
    const matchingProviders = providers.filter((item) => item.appType === provider);
    const activeProviders = matchingProviders.filter((item) => item.isActive);
    const sourceProviders = activeProviders.length > 0 ? activeProviders : matchingProviders;

    return mergeModelLists([
        sourceProviders.flatMap((item) => (
            PROVIDER_MODEL_FIELDS.flatMap((field) => {
                const id = normalizeModelId(item[field]);
                return id ? [modelFromId(id, item.name)] : [];
            })
        )),
    ]);
}

export function buildChatModelList(
    provider: ChatProviderId,
    providers: Provider[] = [],
    options: BuildChatModelListOptions = {},
): ModelInfo[] {
    const storage = options.storage === undefined ? defaultStorage() : options.storage;

    return mergeModelLists([
        providerModelsFor(provider, providers),
        loadStoredChatModels(provider, storage),
        getFallbackChatModels(provider),
    ]);
}

export function ensureChatModelInList(models: ModelInfo[], selectedModel: string): ModelInfo[] {
    const id = normalizeModelId(selectedModel);
    if (!id || models.some((model) => model.id === id)) return models;
    return [modelFromId(id), ...models];
}

export function isChatModelStorageKey(key: string | null): boolean {
    return Boolean(key?.startsWith(CHAT_CUSTOM_MODEL_STORAGE_PREFIX));
}
