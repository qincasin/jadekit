import type {ChatProviderId} from './constants';
import {type BrandGlyph, BrandGlyphIcon} from '../../common/BrandGlyphIcon';

interface ModelIconSpec {
    kind: string;
    glyph: BrandGlyph;
    className: string;
}

interface ModelIconProps {
    provider: ChatProviderId;
    modelId?: string;
    size?: number;
}

interface ProviderBrandIconProps {
    provider: ChatProviderId | 'gemini';
    size?: number;
    colored?: boolean;
}

export function ProviderBrandIcon({provider, size = 16, colored = false}: ProviderBrandIconProps) {
    if (provider === 'gemini') {
        return (
            <span
                aria-label="Gemini provider"
                className={`inline-flex shrink-0 items-center justify-center ${colored ? 'text-[#4285F4]' : 'text-base-content/80'}`}
                data-chat-provider-icon={provider}
                title="Gemini"
            >
                <BrandGlyphIcon glyph="gemini-google" size={size} colored={colored} providerIcon />
            </span>
        );
    }

    const isClaude = provider === 'claude';
    const label = isClaude ? 'Claude Code' : 'Codex';
    const glyph: BrandGlyph = isClaude ? 'claude-lobehub' : 'codex-openai';
    const colorClass = isClaude && colored ? 'text-[#d97757]' : 'text-base-content/80';

    return (
        <span
            aria-label={`${label} provider`}
            className={`inline-flex shrink-0 items-center justify-center ${colorClass}`}
            data-chat-provider-icon={provider}
            title={label}
        >
            <BrandGlyphIcon glyph={glyph} size={size} colored={colored} providerIcon />
        </span>
    );
}

function getClaudeModelIconSpec(modelId: string): ModelIconSpec {
    const lower = modelId.toLowerCase();
    const base = {
        glyph: 'claude-lobehub' as const,
        className: 'text-[#d97757]',
    };
    if (lower.includes('opus')) {
        return {
            kind: 'claude-opus',
            ...base,
        };
    }
    if (lower.includes('sonnet')) {
        return {
            kind: 'claude-sonnet',
            ...base,
        };
    }
    if (lower.includes('haiku')) {
        return {
            kind: 'claude-haiku',
            ...base,
        };
    }
    if (lower.includes('fable')) {
        return {
            kind: 'claude-fable',
            ...base,
        };
    }
    return {
        kind: 'claude-custom',
        ...base,
    };
}

function getCodexModelIconSpec(modelId: string): ModelIconSpec {
    const lower = modelId.toLowerCase();
    const base = {
        glyph: 'codex-openai' as const,
        className: 'text-emerald-600 dark:text-emerald-400',
    };
    if (lower.includes('codex')) {
        return {
            kind: 'codex-codex',
            ...base,
        };
    }
    if (lower.includes('gpt')) {
        return {
            kind: 'codex-gpt',
            ...base,
        };
    }
    return {
        kind: 'codex-custom',
        ...base,
    };
}

export function getChatModelIconKind(provider: ChatProviderId, modelId = ''): string {
    return provider === 'codex'
        ? getCodexModelIconSpec(modelId).kind
        : getClaudeModelIconSpec(modelId).kind;
}

export function ModelIcon({provider, modelId = '', size = 14}: ModelIconProps) {
    const spec = provider === 'codex'
        ? getCodexModelIconSpec(modelId)
        : getClaudeModelIconSpec(modelId);

    return (
        <span
            aria-label={`${provider} model`}
            className={`chat-model-icon-box inline-flex shrink-0 items-center justify-center leading-none ${spec.className}`}
            data-chat-model-icon={spec.kind}
            data-chat-model-icon-glyph={spec.glyph}
            style={{width: size, height: size}}
            title={modelId || provider}
        >
            <BrandGlyphIcon glyph={spec.glyph} size={size} colored={provider === 'claude'} />
        </span>
    );
}
