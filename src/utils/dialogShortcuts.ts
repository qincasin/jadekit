export interface ShortcutTargetLike {
    tagName?: string;
    isContentEditable?: boolean;
    closest?: (selector: string) => unknown;
}

export interface DialogSubmissionRef {
    current: boolean;
}

export function isEditableShortcutTarget(target: EventTarget | null): boolean {
    const element = target as ShortcutTargetLike | null;
    const tagName = element?.tagName?.toLowerCase();
    return (
        tagName === 'input' ||
        tagName === 'textarea' ||
        tagName === 'select' ||
        element?.isContentEditable === true ||
        Boolean(element?.closest?.('[contenteditable="true"]'))
    );
}

export function isEnterShortcutControl(target: EventTarget | null): boolean {
    const element = target as ShortcutTargetLike | null;
    const tagName = element?.tagName?.toLowerCase();
    return tagName === 'button' || isEditableShortcutTarget(target);
}

export function markDialogSubmitted(
    submitted: DialogSubmissionRef,
    onFirstSubmit?: () => void,
): boolean {
    if (submitted.current) return false;
    submitted.current = true;
    onFirstSubmit?.();
    return true;
}
