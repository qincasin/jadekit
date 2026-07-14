import { useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useProviderStore } from '../../stores/useProviderStore';
import { rosterPicksFromProviders } from '../chat/fanout/roster';
import { ShieldCheck, Cpu, CheckSquare, Square, Users } from 'lucide-react';

interface RosterPanelProps {
  selectedPicks: string[];
  onChange: (picks: string[]) => void;
}

export default function RosterPanel({ selectedPicks, onChange }: RosterPanelProps) {
  const { t } = useTranslation();
  const { providers, hasLoaded, loadAllProviders } = useProviderStore();

  useEffect(() => {
    if (!hasLoaded) {
      void loadAllProviders();
    }
  }, [hasLoaded, loadAllProviders]);

  const roster = useMemo(() => rosterPicksFromProviders(providers), [providers]);

  // Handle toggling a single agent
  const handleToggle = (providerId: string) => {
    if (selectedPicks.includes(providerId)) {
      onChange(selectedPicks.filter((id) => id !== providerId));
    } else {
      onChange([...selectedPicks, providerId]);
    }
  };

  // Toggle all option
  const handleToggleAll = () => {
    if (selectedPicks.length === roster.length) {
      onChange([]);
    } else {
      onChange(roster.map((p) => p.providerId));
    }
  };

  return (
    <div className="flex flex-col gap-2 rounded-lg border border-base-300 bg-base-100/50 p-3">
      <div className="flex items-center justify-between border-b border-base-300 pb-2">
        <div className="flex items-center gap-2 text-xs font-semibold text-base-content/85">
          <Users className="h-4 w-4 text-primary" />
          <span>
            {t('helm.composer.rosterTitle', 'Fleet Roster / 舰队成员')} ({selectedPicks.length}/{roster.length})
          </span>
        </div>
        {roster.length > 0 && (
          <button
            type="button"
            onClick={handleToggleAll}
            className="text-[10px] font-medium text-primary hover:underline focus-visible:outline-none"
          >
            {selectedPicks.length === roster.length
              ? t('helm.composer.deselectAll', 'Deselect All / 全不选')
              : t('helm.composer.selectAll', 'Select All / 全选')}
          </button>
        )}
      </div>

      {roster.length === 0 ? (
        <div className="py-4 text-center text-xs text-base-content/50">
          {t('helm.composer.noProviders', 'No available chat providers configured / 无可用大模型提供商')}
        </div>
      ) : (
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3 max-h-36 overflow-y-auto pr-1">
          {roster.map((pick) => {
            const isSelected = selectedPicks.includes(pick.providerId);
            const defaultModel = pick.models[0]?.id || 'default';
            const originalProvider = providers.find((p) => p.id === pick.providerId);
            const isOfficial = originalProvider?.isActive;

            return (
              <div
                key={pick.providerId}
                onClick={() => handleToggle(pick.providerId)}
                className={`flex cursor-pointer items-center justify-between rounded-md border p-2 transition-all duration-200 hover:bg-base-200/50 ${
                  isSelected
                    ? 'border-primary bg-primary/5 text-primary-content'
                    : 'border-base-300 bg-base-200/20 text-base-content'
                }`}
              >
                <div className="flex min-w-0 items-center gap-2">
                  <div className="flex-shrink-0">
                    {isSelected ? (
                      <CheckSquare className="h-4 w-4 text-primary" />
                    ) : (
                      <Square className="h-4 w-4 text-base-content/40" />
                    )}
                  </div>
                  <div className="flex flex-col min-w-0">
                    <span className="truncate text-xs font-medium">
                      {pick.providerName}
                    </span>
                    <span className="truncate text-[10px] text-base-content/60 flex items-center gap-1">
                      <Cpu className="h-3 w-3 inline" />
                      {defaultModel}
                    </span>
                  </div>
                </div>

                {isOfficial && (
                  <span title="Official Provider" className="flex-shrink-0 ml-1">
                    <ShieldCheck className="h-3.5 w-3.5 text-success" />
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
