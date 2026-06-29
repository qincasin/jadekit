import React from 'react';
import { dotVisualFor } from './agentStateDot';
import { AgentStatus } from '../../types/hermes';

export interface HermesAgentStateDotProps {
  status: AgentStatus | string;
  className?: string;
}

export const HermesAgentStateDot: React.FC<HermesAgentStateDotProps> = ({
  status,
  className = '',
}) => {
  const { kind, tone } = dotVisualFor(status);

  // Return the appropriate visual elements based on kind and tone
  const renderVisual = () => {
    switch (kind) {
      case 'spinner':
        return (
          <div className="relative inline-flex items-center justify-center h-4 w-4">
            {/* Subtle dark outline shadow for light themes */}
            <svg
              className="animate-spin motion-reduce:animate-none h-3.5 w-3.5 text-amber-700 border border-amber-800 rounded-full shadow-[0_0_0_1px_rgba(0,0,0,0.15)]"
              viewBox="0 0 24 24"
              fill="none"
              aria-hidden="true"
            >
              <circle
                className="opacity-25"
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                strokeWidth="4"
              />
              <path
                className="opacity-75"
                fill="currentColor"
                d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
              />
            </svg>
          </div>
        );

      case 'dot':
        if (tone === 'amber') {
          return (
            <div
              className="inline-flex items-center justify-center h-4 w-4 rounded-full bg-amber-500 border border-white text-[10px] leading-none font-bold text-white shadow-sm"
              aria-hidden="true"
            >
              !
            </div>
          );
        }
        // Fallback dot
        return (
          <div
            className="inline-block h-3.5 w-3.5 rounded-full bg-amber-500 border border-amber-700"
            aria-hidden="true"
          />
        );

      case 'check':
        return (
          <div
            className="inline-flex items-center justify-center h-4 w-4 rounded-full border border-emerald-500 text-emerald-500 text-[10px] leading-none font-bold font-mono"
            aria-hidden="true"
          >
            ✓
          </div>
        );

      case 'square':
        return (
          <div
            className="inline-flex items-center justify-center h-4 w-4 rounded-sm bg-red-500 border border-red-700 text-[8px] leading-none font-bold font-mono text-white"
            aria-hidden="true"
          >
            ▢
          </div>
        );

      default:
        return (
          <div
            className="inline-block h-3.5 w-3.5 rounded-full bg-amber-500 border border-amber-700"
            aria-hidden="true"
          />
        );
    }
  };

  return (
    <span className={`inline-flex items-center ${className}`}>
      {renderVisual()}
      <span className="sr-only">{status}</span>
    </span>
  );
};

export default HermesAgentStateDot;
