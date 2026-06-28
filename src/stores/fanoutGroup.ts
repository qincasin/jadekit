export interface FanoutGroupTabLike {
    fanoutGroupId?: string | null;
}

export function fanoutTabsOf<T extends FanoutGroupTabLike>(tabs: T[], groupId: string | null | undefined): T[] {
    if (!groupId) return [];
    return tabs.filter((tab) => tab.fanoutGroupId === groupId);
}
