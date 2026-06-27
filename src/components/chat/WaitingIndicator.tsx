import {useTranslation} from 'react-i18next';
import BouncingDots from './BouncingDots';

function translateWithFallback(t: (key: string) => string, key: string, fallback: string): string {
    const translated = t(key);
    return translated === key ? fallback : translated;
}

export default function WaitingIndicator() {
    const { t } = useTranslation();
    const waitingLabel = translateWithFallback(t, 'chat.message.waiting', 'Waiting for response...');

    return (
        <div className="mx-auto flex w-full max-w-4xl items-center gap-2 px-3 py-2 text-sm text-base-content/50" aria-live="polite">
            <BouncingDots size={4} />
            <span>{waitingLabel}</span>
        </div>
    );
}
