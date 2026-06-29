import React from 'react';
import { Trophy, AlertCircle, Award } from 'lucide-react';
import { JudgeVerdictDto } from '../../types/hermes';
import { cn } from '../../utils/cn';

interface JudgeVerdictCardProps {
  verdict: JudgeVerdictDto | null;
}

export const JudgeVerdictCard: React.FC<JudgeVerdictCardProps> = ({ verdict }) => {
  if (!verdict) {
    return (
      <div className="flex flex-col items-center justify-center p-8 rounded-xl border border-dashed border-base-300 bg-base-100/50 text-center text-base-content/60 gap-3">
        <AlertCircle className="h-6 w-6 text-base-content/40" />
        <p className="text-xs leading-relaxed font-medium">
          评判结果将在引擎接通后显示 <br />
          <span className="text-[10px] opacity-75 font-normal">
            Verdicts will be displayed after engine bridge is implemented.
          </span>
        </p>
      </div>
    );
  }

  const { winnerIndex, scores, reason, candidates } = verdict;

  return (
    <div className="flex flex-col gap-4 p-5 rounded-xl border border-base-300 bg-base-100/70 shadow-sm transition-all">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-base-200 pb-3">
        <div className="flex items-center gap-2">
          <Award className="h-5 w-5 text-primary" />
          <h4 className="text-sm font-bold text-base-content">
            智能评判决策 / Judge Verdict
          </h4>
        </div>
        <span className="px-2 py-0.5 text-[10px] font-semibold bg-primary/10 text-primary rounded-full uppercase tracking-wider">
          Decision Completed
        </span>
      </div>

      {/* Candidates List */}
      <div className="flex flex-col gap-3">
        {candidates.map((candidate) => {
          const score = scores[candidate.index] ?? 0;
          const isWinner = candidate.index === winnerIndex;

          return (
            <div
              key={candidate.index}
              className={cn(
                "flex flex-col p-3 rounded-lg border transition-all",
                isWinner
                  ? "border-amber-300 dark:border-amber-900 bg-amber-50/20 dark:bg-amber-950/10 shadow-[0_0_8px_rgba(245,158,11,0.08)]"
                  : "border-base-200 bg-base-200/30 hover:border-base-300"
              )}
            >
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2">
                  <span className="flex items-center justify-center h-5 w-5 rounded-full text-[10px] font-bold bg-base-300 text-base-content/80">
                    #{candidate.index}
                  </span>
                  <span className="text-xs font-mono font-medium text-base-content/90">
                    {candidate.agentId}
                  </span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-xs font-mono font-bold text-base-content">
                    {score} pts
                  </span>
                  {isWinner && (
                    <span className="flex items-center gap-1 px-1.5 py-0.5 text-[9px] font-bold bg-amber-500 text-white rounded uppercase tracking-wide">
                      <Trophy className="h-3 w-3 fill-current" />
                      Winner
                    </span>
                  )}
                </div>
              </div>

              {/* Progress bar */}
              <div className="w-full bg-base-300/40 rounded-full h-1.5 overflow-hidden">
                <div
                  className={cn(
                    "h-full rounded-full transition-all duration-500",
                    isWinner ? "bg-amber-500" : "bg-primary/60"
                  )}
                  style={{ width: `${Math.min(100, Math.max(0, score))}%` }}
                />
              </div>
            </div>
          );
        })}
      </div>

      {/* Reason */}
      {reason && (
        <div className="flex flex-col gap-1.5 p-3 rounded-lg bg-base-200/50 border border-base-200 text-xs leading-relaxed text-base-content/85">
          <span className="font-semibold text-[10px] text-base-content/50 uppercase tracking-wider">
            决策理由 / Evaluation Reason:
          </span>
          <p className="whitespace-pre-line select-text font-medium">{reason}</p>
        </div>
      )}
    </div>
  );
};

export default JudgeVerdictCard;
