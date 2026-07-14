import type {ChatProviderId} from '../composer/constants';
import {getFallbackChatModels} from '../../../utils/chatModels';

export interface FanoutRosterProvider {
    id: string;
    name: string;
    appType: string;
    isActive?: boolean;
    defaultSonnetModel?: string;
    defaultOpusModel?: string;
    defaultHaikuModel?: string;
    defaultReasoningModel?: string;
}

export interface FanoutRosterModel {
    id: string;
    label: string;
}

export interface FanoutRosterPick {
    providerId: string;
    providerName: string;
    chatProvider: ChatProviderId;
    models: FanoutRosterModel[];
}

function isChatProviderId(value: string): value is ChatProviderId {
    return value === 'claude' || value === 'codex';
}

function addModel(models: FanoutRosterModel[], seen: Set<string>, id: string | undefined): void {
    const normalized = id?.trim();
    if (!normalized || seen.has(normalized)) return;
    seen.add(normalized);
    models.push({id: normalized, label: normalized});
}

function modelsForProvider(provider: FanoutRosterProvider): FanoutRosterModel[] {
    if (!isChatProviderId(provider.appType)) return [];

    const seen = new Set<string>();
    const models: FanoutRosterModel[] = [];
    addModel(models, seen, provider.defaultOpusModel);
    addModel(models, seen, provider.defaultReasoningModel);
    addModel(models, seen, provider.defaultSonnetModel);
    addModel(models, seen, provider.defaultHaikuModel);

    for (const model of getFallbackChatModels(provider.appType)) {
        if (seen.has(model.id)) continue;
        seen.add(model.id);
        models.push({id: model.id, label: model.label});
    }

    return models;
}

export function rosterPicksFromProviders(providers: FanoutRosterProvider[]): FanoutRosterPick[] {
    return providers.flatMap((provider) => {
        if (!isChatProviderId(provider.appType)) return [];
        return [{
            providerId: provider.id,
            providerName: provider.name,
            chatProvider: provider.appType,
            models: modelsForProvider(provider),
        }];
    });
}
