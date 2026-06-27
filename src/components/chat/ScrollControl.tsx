import {ArrowDown} from 'lucide-react';
import {useTranslation} from 'react-i18next';
import {getChatNavigationControlLabel} from '../../utils/chatUiBehavior';

interface ScrollControlProps {
    visible: boolean;
    onScrollToBottom: () => void;
}

export default function ScrollControl({ visible, onScrollToBottom }: ScrollControlProps) {
    const { t } = useTranslation();
    const scrollToBottomLabel = getChatNavigationControlLabel({
        control: 'scroll-to-bottom',
        translate: (key, options) => t(key, options),
    });

    if (!visible) return null;

    return (
        <button
            type="button"
            className="btn btn-circle btn-sm absolute bottom-32 right-4 border border-base-300 bg-base-100/95 shadow-lg backdrop-blur transition hover:scale-105 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary xl:right-60"
            title={scrollToBottomLabel}
            aria-label={scrollToBottomLabel}
            onClick={onScrollToBottom}
        >
            <ArrowDown size={16} />
        </button>
    );
}
