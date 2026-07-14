import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, Check, X, Loader2 } from 'lucide-react';
import { InterventionGateDto } from '../../types/hermes';
import { gateResolve } from '../../services/hermesService';

export interface InterventionGateCardProps {
  gate: InterventionGateDto;
  onResolve?: (resolution: 'approve' | 'reject', comment: string) => void;
}

export const InterventionGateCard: React.FC<InterventionGateCardProps> = ({
  gate,
  onResolve,
}) => {
  const { t } = useTranslation();
  const [comment, setComment] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async (resolution: 'approve' | 'reject') => {
    setIsSubmitting(true);
    try {
      await gateResolve(gate.id, resolution, comment);
      if (onResolve) {
        onResolve(resolution, comment);
      }
    } catch (err) {
      console.error('Failed to resolve intervention gate:', err);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="border border-amber-300 dark:border-amber-900 bg-amber-50/50 dark:bg-amber-950/15 rounded-xl p-5 shadow-sm flex flex-col gap-4 max-w-2xl mx-auto w-full transition-all duration-300">
      <div className="flex items-start gap-3">
        <AlertTriangle className="h-5 w-5 text-amber-500 flex-shrink-0 mt-0.5" />
        <div className="flex-1 min-w-0">
          <h4 className="text-sm font-semibold text-amber-800 dark:text-amber-300">
            {t('helm.attentionGate.title', '需要人工介入 / Attention Required')}
          </h4>
          <p className="text-xs text-amber-700 dark:text-amber-400/90 mt-1 leading-relaxed">
            {t(
              'helm.attentionGate.description',
              '该 Agent 已暂停，需要您进行评审或提供输入以继续执行。 / This agent has paused and requires your review or intervention to proceed.'
            )}
          </p>
        </div>
      </div>

      {gate.question && (
        <div className="bg-amber-100/55 dark:bg-amber-950/30 rounded-lg p-3 border border-amber-200/50 dark:border-amber-900/30">
          <span className="text-[10px] font-semibold text-amber-800/60 dark:text-amber-400/60 uppercase tracking-wider block mb-1">
            {t('helm.attentionGate.interventionDetails', '介入决策详情 / Intervention Details')}
          </span>
          <p className="text-xs text-amber-900 dark:text-amber-200 font-medium">
            {gate.question}
          </p>
        </div>
      )}

      <div className="flex flex-col gap-2">
        <label className="text-[10px] font-semibold text-base-content/60 uppercase tracking-wider">
          {t('helm.attentionGate.guidanceInput', '手动输入指导意见/备注 / Manual Guidance / Comment')}
        </label>
        <textarea
          value={comment}
          onChange={(e) => setComment(e.target.value)}
          disabled={isSubmitting}
          placeholder={t(
            'helm.attentionGate.guidancePlaceholder',
            '请输入指导意见或备注... / Enter manual guidance or comment...'
          )}
          className="textarea textarea-bordered textarea-xs w-full h-20 rounded-lg focus:outline-none focus:border-amber-400 font-sans text-xs bg-base-100 text-base-content"
        />
      </div>

      <div className="flex items-center justify-between border-t border-amber-200 dark:border-amber-900/40 pt-4 mt-2">
        <span className="text-[10px] font-semibold text-amber-800/60 dark:text-amber-400/60 uppercase tracking-wider">
          {t('helm.attentionGate.actionRequired', '需采取行动 / Action Required')}
        </span>
        <div className="flex gap-2">
          <button
            onClick={() => handleSubmit('reject')}
            disabled={isSubmitting}
            className="btn btn-error btn-xs rounded-lg font-semibold flex items-center gap-1 text-white bg-red-600 hover:bg-red-700 border-none"
          >
            {isSubmitting ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <X className="h-3 w-3" />
            )}
            {t('helm.attentionGate.reject', 'Reject / 拒绝')}
          </button>
          <button
            onClick={() => handleSubmit('approve')}
            disabled={isSubmitting}
            className="btn btn-success btn-xs rounded-lg font-semibold flex items-center gap-1 text-white bg-emerald-600 hover:bg-emerald-700 border-none"
          >
            {isSubmitting ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Check className="h-3 w-3" />
            )}
            {t('helm.attentionGate.approve', 'Approve / 批准')}
          </button>
        </div>
      </div>
    </div>
  );
};
