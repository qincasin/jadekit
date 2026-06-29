import { describe, it, expect } from 'vitest';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { JudgeVerdictCard } from './JudgeVerdictCard';
import { JudgeVerdictDto } from '../../types/hermes';

describe('JudgeVerdictCard', () => {
  it('renders empty banner when verdict is null', () => {
    const html = renderToString(React.createElement(JudgeVerdictCard, { verdict: null }));
    expect(html).toContain('评判结果将在引擎接通后显示');
    expect(html).toContain('Verdicts will be displayed after engine bridge is implemented.');
  });

  it('renders candidates, scores, highlights winner, and shows reason when verdict is provided', () => {
    const mockVerdict: JudgeVerdictDto = {
      winnerIndex: 1,
      scores: [85, 95, 70],
      reason: 'Candidate 1 showed superior performance in code safety and speed.',
      candidates: [
        { index: 0, agentId: 'agent_alpha' },
        { index: 1, agentId: 'agent_beta' },
        { index: 2, agentId: 'agent_gamma' },
      ],
    };

    const html = renderToString(React.createElement(JudgeVerdictCard, { verdict: mockVerdict }));
    
    // Check candidate lists / agent IDs are rendered
    expect(html).toContain('agent_alpha');
    expect(html).toContain('agent_beta');
    expect(html).toContain('agent_gamma');

    // Check scores are rendered
    expect(html).toContain('85');
    expect(html).toContain('95');
    expect(html).toContain('70');

    // Check reason is rendered
    expect(html).toContain('Candidate 1 showed superior performance in code safety and speed.');

    // Winner highlight/trophy check.
    expect(html.toLowerCase()).toContain('trophy');
  });
});
