import React, { useState, useEffect, useRef } from 'react';
import { useHermesStore } from '../../stores/useHermesStore';
import { filterAgents } from './jumpSearch';
import { Search, Terminal, Sparkles } from 'lucide-react';
import { HermesAgentStateDot } from './HermesAgentStateDot';

interface JumpPaletteProps {
  isOpen: boolean;
  onClose: () => void;
}

export const JumpPalette: React.FC<JumpPaletteProps> = ({ isOpen, onClose }) => {
  const agentsMap = useHermesStore((state) => state.agents);
  const setSelectedAgentId = useHermesStore((state) => state.setSelectedAgentId);
  const selectedAgentId = useHermesStore((state) => state.selectedAgentId);

  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const agentsList = Object.values(agentsMap);
  const filtered = filterAgents(agentsList, query);

  // Auto-focus input on open
  useEffect(() => {
    if (isOpen) {
      setQuery('');
      setSelectedIndex(0);
      setTimeout(() => {
        inputRef.current?.focus();
      }, 50);
    }
  }, [isOpen]);

  // Handle keys inside the input/palette
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelectedIndex((prev) => (filtered.length > 0 ? (prev + 1) % filtered.length : 0));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelectedIndex((prev) => (filtered.length > 0 ? (prev - 1 + filtered.length) % filtered.length : 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        if (filtered.length > 0 && selectedIndex >= 0 && selectedIndex < filtered.length) {
          const selectedAgent = filtered[selectedIndex];
          setSelectedAgentId(selectedAgent.id);
          onClose();
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [isOpen, filtered, selectedIndex, onClose, setSelectedAgentId]);

  // Adjust selected index if it exceeds list length
  useEffect(() => {
    if (selectedIndex >= filtered.length) {
      setSelectedIndex(0);
    }
  }, [filtered.length, selectedIndex]);

  // Click outside to close
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        onClose();
      }
    };
    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
    }
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-24 bg-black/40 backdrop-blur-[4px] animate-in fade-in duration-200">
      <div
        ref={containerRef}
        className="w-full max-w-xl bg-base-100/90 dark:bg-neutral/90 border border-base-300 dark:border-neutral-focus rounded-2xl shadow-2xl overflow-hidden transform transition-all scale-100 animate-in zoom-in-95 duration-200 flex flex-col max-h-[480px]"
      >
        {/* Search Input Bar */}
        <div className="flex items-center gap-3 px-4 py-3.5 border-b border-base-200 dark:border-neutral-focus">
          <Search className="w-5 h-5 text-base-content/40 flex-shrink-0" />
          <input
            ref={inputRef}
            type="text"
            className="flex-1 bg-transparent text-sm text-base-content outline-none placeholder-base-content/40"
            placeholder="搜索代理 ID, 任务 ID 或状态... / Search agents..."
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedIndex(0);
            }}
          />
          <kbd className="kbd kbd-sm font-mono text-[10px] opacity-50 flex-shrink-0">ESC</kbd>
        </div>

        {/* Results List */}
        <div className="flex-1 overflow-y-auto p-2 scrollbar-thin">
          {filtered.length === 0 ? (
            <div className="p-8 text-center text-xs text-base-content/40 flex flex-col items-center justify-center gap-2">
              <Sparkles className="w-8 h-8 opacity-40 animate-pulse text-primary" />
              <span>未找到匹配的代理 / No agents found</span>
            </div>
          ) : (
            <ul className="space-y-1">
              {filtered.map((agent, index) => {
                const isHighlighted = index === selectedIndex;
                const isSelected = selectedAgentId === agent.id;

                return (
                  <li
                    key={agent.id}
                    className={`flex items-center justify-between px-3 py-2.5 rounded-xl cursor-pointer transition-all duration-150 ${
                      isHighlighted
                        ? 'bg-primary/10 text-primary border-l-4 border-primary pl-2 shadow-sm'
                        : isSelected
                        ? 'bg-base-200 text-base-content border-l-4 border-base-300 pl-2'
                        : 'hover:bg-base-200/50 text-base-content border-l-4 border-transparent'
                    }`}
                    onClick={() => {
                      setSelectedAgentId(agent.id);
                      onClose();
                    }}
                    onMouseEnter={() => setSelectedIndex(index)}
                  >
                    <div className="flex items-center gap-3 min-w-0">
                      <HermesAgentStateDot
                        status={agent.status}
                        className="flex-shrink-0"
                      />
                      <div className="flex flex-col min-w-0">
                        <div className="flex items-center gap-2">
                          <span className={`font-semibold text-xs truncate ${isSelected ? 'text-primary' : ''}`}>
                            {agent.id}
                          </span>
                          {agent.status === 'needs-attention' && (
                            <span className="badge badge-warning badge-xs font-semibold px-1 py-0.5 rounded text-[9px] uppercase tracking-wider">
                              Attention
                            </span>
                          )}
                        </div>
                        <span className="text-[10px] text-base-content/50 truncate flex items-center gap-1">
                          {agent.taskId ? (
                            <>
                              <Terminal className="w-3 h-3 flex-shrink-0" />
                              <span>Task: {agent.taskId}</span>
                            </>
                          ) : (
                            'No active task'
                          )}
                        </span>
                      </div>
                    </div>

                    <div className="flex items-center gap-2 flex-shrink-0 pl-2">
                      <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-base-300/40 text-base-content/70">
                        {agent.status}
                      </span>
                      {isHighlighted && (
                        <span className="text-[10px] text-primary/70 flex items-center gap-0.5">
                          <kbd className="kbd kbd-xs py-0.5 font-mono text-[9px]">↵</kbd>
                        </span>
                      )}
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        {/* Footer info bar */}
        <div className="bg-base-200/50 dark:bg-neutral-focus/20 px-4 py-2 border-t border-base-200 dark:border-neutral-focus flex justify-between items-center text-[10px] text-base-content/40">
          <div className="flex items-center gap-2">
            <span>使用 <kbd className="kbd kbd-xs">↓</kbd> <kbd className="kbd kbd-xs">↑</kbd> 导航</span>
            <span>•</span>
            <span><kbd className="kbd kbd-xs">Enter</kbd> 选择</span>
          </div>
          <div>
            <span>共 {filtered.length} 个代理 / {filtered.length} agents</span>
          </div>
        </div>
      </div>
    </div>
  );
};
