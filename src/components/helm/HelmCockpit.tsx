import { useEffect, useState } from 'react';
import { useCockpitLayout } from './useCockpitLayout';
import FleetKanban from './FleetKanban';
import { JumpPalette } from './JumpPalette';
import SessionPanel from './SessionPanel';
import HelmComposer from './HelmComposer';
import { InspectorPanel } from './InspectorPanel';
import { useHermesStore } from '../../stores/useHermesStore';

export default function HelmCockpit() {
  const {
    leftOpen,
    rightOpen,
    toggleLeft,
    toggleRight,
  } = useCockpitLayout();

  const [isPaletteOpen, setIsPaletteOpen] = useState(false);

  useEffect(() => {
    let active = true;
    let cleanup: (() => void) | undefined;
    const store = useHermesStore.getState();

    void store.refreshSnapshot();
    void store.subscribeEvents().then((unsubscribe) => {
      if (active) {
        cleanup = unsubscribe;
      } else {
        unsubscribe();
      }
    });

    return () => {
      active = false;
      cleanup?.();
    };
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Alt+[
      if (e.altKey && e.key === '[') {
        e.preventDefault();
        toggleLeft();
      }
      // Alt+]
      if (e.altKey && e.key === ']') {
        e.preventDefault();
        toggleRight();
      }
      // Meta+K or Ctrl+K
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setIsPaletteOpen(true);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [toggleLeft, toggleRight]);

  return (
    <div className="flex flex-1 overflow-hidden bg-[#FAFBFC] dark:bg-[#0d1117] text-base-content h-full">
      {/* Three pane layout using CSS Grid */}
      <div
        className="grid h-full w-full overflow-hidden transition-all duration-300"
        style={{
          gridTemplateColumns: `${leftOpen ? '320px' : '0px'} 1fr ${rightOpen ? '320px' : '0px'}`,
        }}
      >
        {/* Left Panel */}
        <aside
          className={`flex flex-col border-r border-base-300 bg-base-100 overflow-hidden transition-all duration-300 ${
            leftOpen ? 'opacity-100' : 'opacity-0 pointer-events-none'
          }`}
          aria-label="舰队看板 (Left Panel)"
        >
          <div className="p-4 border-b border-base-300 flex justify-between items-center bg-base-200 flex-shrink-0">
            <span className="font-bold text-sm tracking-wider uppercase text-base-content/80">舰队看板 / Fleet Kanban</span>
            <button
              onClick={toggleLeft}
              className="btn btn-ghost btn-xs h-6 min-h-0 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
              aria-label="收起左栏"
            >
              ◀
            </button>
          </div>
          <div className="flex-1 overflow-hidden flex flex-col min-h-0">
            <FleetKanban />
          </div>
        </aside>

        {/* Center Panel */}
        <section
          className="flex flex-col bg-base-100 overflow-hidden"
          aria-label="工作区会话 (Center Panel)"
        >
          <div className="p-4 border-b border-base-300 flex justify-between items-center bg-base-200">
            <div className="flex items-center gap-2">
              {!leftOpen && (
                <button
                  onClick={toggleLeft}
                  className="btn btn-ghost btn-xs h-6 min-h-0 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
                  aria-label="展开左栏"
                >
                  ▶
                </button>
              )}
              <span className="font-bold text-sm tracking-wider uppercase text-base-content/80">Center Panel</span>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-xs text-base-content/50">Press <kbd className="kbd kbd-sm font-mono text-[10px]">Alt+[</kbd> / <kbd className="kbd kbd-sm font-mono text-[10px]">Alt+]</kbd> to toggle</span>
              {!rightOpen && (
                <button
                  onClick={toggleRight}
                  className="btn btn-ghost btn-xs h-6 min-h-0 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
                  aria-label="展开右栏"
                >
                  ◀
                </button>
              )}
            </div>
          </div>
          <div className="flex-1 min-h-0 flex flex-col">
            <SessionPanel />
          </div>
          <div className="p-4 border-t border-base-300 bg-base-100 flex-shrink-0">
            <HelmComposer />
          </div>
        </section>

        {/* Right Panel */}
        <aside
          className={`flex flex-col border-l border-base-300 bg-base-100 overflow-hidden transition-all duration-300 ${
            rightOpen ? 'opacity-100' : 'opacity-0 pointer-events-none'
          }`}
          aria-label="检查器 (Right Panel)"
        >
          <InspectorPanel onClose={toggleRight} />
        </aside>
      </div>
      <JumpPalette isOpen={isPaletteOpen} onClose={() => setIsPaletteOpen(false)} />
    </div>
  );
}
