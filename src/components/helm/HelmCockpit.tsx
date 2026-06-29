import { useEffect, useState } from 'react';
import { useCockpitLayout } from './useCockpitLayout';
import FleetKanban from './FleetKanban';
import { JumpPalette } from './JumpPalette';

export default function HelmCockpit() {
  const {
    leftOpen,
    rightOpen,
    toggleLeft,
    toggleRight,
  } = useCockpitLayout();

  const [isPaletteOpen, setIsPaletteOpen] = useState(false);

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
          <div className="p-6 overflow-y-auto flex-1 flex flex-col justify-between">
            <div>
              <h2 className="text-lg font-bold mb-2">主工作区会话 (Worker Transcript)</h2>
              <div className="bg-base-200 border border-base-300 rounded p-4 text-xs font-mono leading-relaxed max-w-2xl text-base-content/70">
                <div className="text-base-content/40 mb-2">
                  // 顶部桥接提示小字 (严格同步中英)
                </div>
                <div>完整会话由 Phase 3.5 worker-transcript 桥提供；实现阶段桥未接通前显示活动流回退态，不伪造 transcript。</div>
                <div className="mt-2 text-base-content/40">
                  Full transcripts are provided by Phase 3.5 worker-transcript bridge. Falling back to simple activity stream during early implementation.
                </div>
              </div>
            </div>
            <div className="border-t border-base-300 pt-4 bg-base-100">
              <p className="text-xs text-base-content/50">这里是 Composer / 派发输入框占位区域</p>
            </div>
          </div>
        </section>

        {/* Right Panel */}
        <aside
          className={`flex flex-col border-l border-base-300 bg-base-100 overflow-hidden transition-all duration-300 ${
            rightOpen ? 'opacity-100' : 'opacity-0 pointer-events-none'
          }`}
          aria-label="检查器 (Right Panel)"
        >
          <div className="p-4 border-b border-base-300 flex justify-between items-center bg-base-200">
            <button
              onClick={toggleRight}
              className="btn btn-ghost btn-xs h-6 min-h-0 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
              aria-label="收起右栏"
            >
              ▶
            </button>
            <span className="font-bold text-sm tracking-wider uppercase text-base-content/80">Right Panel</span>
          </div>
          <div className="p-4 overflow-y-auto flex-1">
            <p className="text-xs text-base-content/60 mb-2">检查器 / Inspector</p>
            <div className="text-xs text-base-content/40">审查 retained worktree 改动与合并</div>
            <div className="mt-8 border border-dashed border-base-300 rounded p-6 text-center text-xs text-base-content/40">
              尚未选择待评审 worker
            </div>
          </div>
        </aside>
      </div>
      <JumpPalette isOpen={isPaletteOpen} onClose={() => setIsPaletteOpen(false)} />
    </div>
  );
}
