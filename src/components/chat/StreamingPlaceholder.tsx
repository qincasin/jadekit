import {useEffect, useState} from 'react';
import {useTranslation} from 'react-i18next';
import BouncingDots from './BouncingDots';

interface StreamingPlaceholderProps {
    delayMs?: number;
}

function translateWithFallback(t: (key: string) => string, key: string, fallback: string): string {
    const translated = t(key);
    return translated === key ? fallback : translated;
}

export default function StreamingPlaceholder({ delayMs = 350 }: StreamingPlaceholderProps) {
    const { t } = useTranslation();
    const [showConnectedHint, setShowConnectedHint] = useState(false);
    const waitingLabel = translateWithFallback(t, 'chat.message.waiting', 'Waiting for response...');
    const connectedLabel = translateWithFallback(
        t,
        'chat.message.streamingConnected',
        'Connected, generating response...',
    );

    useEffect(() => {
        const timer = window.setTimeout(() => setShowConnectedHint(true), delayMs);
        return () => window.clearTimeout(timer);
    }, [delayMs]);

    return (
        <div className="flex items-center gap-2 text-sm text-base-content/50" aria-live="polite">
            <BouncingDots size={4} />
            <span>
                {showConnectedHint
                    ? connectedLabel
                    : waitingLabel}
            </span>
        </div>
    );
}
