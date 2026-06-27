import type {ImageBlock} from '../types/chat';

function firstNonEmptyString(...values: Array<unknown>): string | undefined {
    for (const value of values) {
        if (typeof value === 'string' && value.trim().length > 0) {
            return value.trim();
        }
    }
    return undefined;
}

function getImageUrlValue(block: ImageBlock): string | undefined {
    if (typeof block.image_url === 'string') return block.image_url;
    return firstNonEmptyString(
        block.image_url?.url,
        block.image_url?.path,
        block.url,
        block.path,
        block.source?.url,
        block.source?.path,
    );
}

function basename(pathOrUrl: string): string {
    const withoutQuery = pathOrUrl.split(/[?#]/, 1)[0] ?? pathOrUrl;
    return withoutQuery.split(/[/\\]/).filter(Boolean).pop() ?? withoutQuery;
}

export function isImageContentBlock(block: {type?: string}): block is ImageBlock {
    return block.type === 'image' || block.type === 'input_image';
}

export function isLikelyImageBase64Text(text: string | undefined): boolean {
    const normalized = text?.trim().replace(/\s+/g, '') ?? '';
    if (normalized.length < 16) return false;
    if (/^data:image\/[a-z0-9.+-]+;base64,/i.test(normalized)) return true;
    if (!/^[A-Za-z0-9+/=]+$/.test(normalized)) return false;

    const imageMarkers = [
        'iVBORw0KGgo',
        '/9j/',
        'R0lGOD',
        'UklGR',
        'AElFTkSuQmCC',
        'SUVORK5CYII',
        'Jggg',
    ];
    if (imageMarkers.some((marker) => normalized.includes(marker))) return true;

    const mixScore = [
        /[A-Z]/.test(normalized),
        /[a-z]/.test(normalized),
        /\d/.test(normalized),
        /[+/]/.test(normalized),
    ].filter(Boolean).length;

    if (/=+$/.test(normalized) && mixScore >= 3) return true;
    return normalized.length >= 120 && mixScore >= 3;
}

export function isImagePlaceholderText(text: string | undefined): boolean {
    const trimmed = text?.trim() ?? '';
    if (!trimmed) return false;
    return /^<image\b[^>]*>\s*(?:<\/image>)?$/i.test(trimmed)
        || /^<\/image>$/i.test(trimmed);
}

export function getImageBlockMediaType(block: ImageBlock): string {
    return firstNonEmptyString(
        block.media_type,
        block.mediaType,
        block.source?.media_type,
        block.source?.mediaType,
    ) ?? 'image/png';
}

export function getImageBlockData(block: ImageBlock): string | undefined {
    return firstNonEmptyString(block.data, block.source?.data);
}

export function getImageBlockUrl(block: ImageBlock): string | undefined {
    return getImageUrlValue(block);
}

export function getImageBlockFileName(block: ImageBlock): string | undefined {
    const explicit = firstNonEmptyString(block.fileName, block.name);
    if (explicit) return basename(explicit);

    const url = getImageUrlValue(block);
    return url ? basename(url) : undefined;
}

export function getImageBlockDataUrl(block: ImageBlock): string | null {
    const data = getImageBlockData(block);
    if (!data) return null;
    if (data.startsWith('data:')) return data;
    return `data:${getImageBlockMediaType(block)};base64,${data}`;
}

export function getImageBlockPreviewText(block: ImageBlock): string {
    const fileName = getImageBlockFileName(block);
    return fileName ? `图片 ${fileName}` : '图片';
}

export function getImageBlockSearchText(block: ImageBlock): string {
    return [
        '图片',
        'image',
        getImageBlockFileName(block),
        getImageBlockMediaType(block),
    ]
        .filter((part): part is string => Boolean(part))
        .join(' ');
}
