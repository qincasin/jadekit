import { vi, describe, it, expect, beforeEach } from 'vitest';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { InterventionGateCard } from './InterventionGateCard';
import { InterventionGateDto } from '../../types/hermes';
import * as hermesService from '../../services/hermesService';

// Setup mock react hooks to simulate input state changes under node environment
const mockStateMap = new Map<number, any>();
let mockStateIndex = 0;
const mockEffects: Array<() => void> = [];

const mockReactHarness = {
  useState: (initialValueOrFn: any) => {
    const currentIndex = mockStateIndex++;
    if (!mockStateMap.has(currentIndex)) {
      const val = typeof initialValueOrFn === 'function' ? initialValueOrFn() : initialValueOrFn;
      mockStateMap.set(currentIndex, val);
    }
    const setter = (newValueOrFn: any) => {
      const currentVal = mockStateMap.get(currentIndex);
      const nextVal = typeof newValueOrFn === 'function' ? newValueOrFn(currentVal) : newValueOrFn;
      mockStateMap.set(currentIndex, nextVal);
      mockEffects.forEach(effect => effect());
    };
    return [mockStateMap.get(currentIndex), setter];
  },
  useEffect: (effect: () => void, _deps?: any[]) => {
    mockEffects.push(effect);
    effect();
  },
  reset: () => {
    mockStateMap.clear();
    mockStateIndex = 0;
    mockEffects.length = 0;
  },
  resetIndex: () => {
    mockStateIndex = 0;
  }
};

vi.mock('react', async (importOriginal) => {
  const original = await importOriginal<typeof import('react')>();
  return {
    ...original,
    useState: (init: any) => mockReactHarness.useState(init),
    useEffect: (eff: any, deps: any) => mockReactHarness.useEffect(eff, deps),
  };
});

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, defaultValue?: string) => defaultValue || key,
  }),
}));


vi.mock('../../services/hermesService', () => {
  return {
    gateResolve: vi.fn(),
  };
});

function findElement(tree: any, predicate: (node: any) => boolean): any {
  if (!tree) return null;
  if (predicate(tree)) return tree;
  if (tree.props && tree.props.children) {
    const children = React.Children.toArray(tree.props.children);
    for (const child of children) {
      const found = findElement(child, predicate);
      if (found) return found;
    }
  }
  return null;
}

describe('InterventionGateCard', () => {
  const mockGate: InterventionGateDto = {
    id: 'gate-task-02',
    taskId: 'task-02',
    question: 'Please review the generated code changes.',
    options: ['approve', 'reject'],
    status: 'pending',
  };

  beforeEach(() => {
    mockReactHarness.reset();
    vi.clearAllMocks();
  });

  it('displays description', () => {
    const html = renderToString(
      React.createElement(InterventionGateCard, { gate: mockGate })
    );
    expect(html).toContain('Please review the generated code changes.');
  });

  it('accepts text comment inputs and executes gateResolve upon approve button click', async () => {
    mockReactHarness.resetIndex();
    const tree = InterventionGateCard({ gate: mockGate });

    const textarea = findElement(tree, (node) => node.type === 'textarea');
    expect(textarea).toBeDefined();

    // Simulate comment typing
    textarea.props.onChange({ target: { value: 'Looks great!' } });

    // Re-render components with mockReactHarness index reset to simulate React re-render
    mockReactHarness.resetIndex();
    const renderedTree = InterventionGateCard({ gate: mockGate });

    const approveBtn = findElement(renderedTree, (node) => 
      node.type === 'button' && 
      (node.props.className?.includes('btn-success') || 
       (Array.isArray(node.props.children) && 
        node.props.children.some((c: any) => c?.toString().includes('Approve'))) ||
       node.props.children?.toString().includes('Approve'))
    );
    expect(approveBtn).toBeDefined();

    // Trigger click handler
    await approveBtn.props.onClick();

    expect(hermesService.gateResolve).toHaveBeenCalledWith('gate-task-02', 'approve', 'Looks great!');
  });

  it('executes gateResolve upon reject button click with comment', async () => {
    mockReactHarness.resetIndex();
    const tree = InterventionGateCard({ gate: mockGate });

    const textarea = findElement(tree, (node) => node.type === 'textarea');
    expect(textarea).toBeDefined();

    textarea.props.onChange({ target: { value: 'Needs revisions.' } });

    mockReactHarness.resetIndex();
    const renderedTree = InterventionGateCard({ gate: mockGate });

    const rejectBtn = findElement(renderedTree, (node) => 
      node.type === 'button' && 
      (node.props.className?.includes('btn-error') || 
       (Array.isArray(node.props.children) && 
        node.props.children.some((c: any) => c?.toString().includes('Reject'))) ||
       node.props.children?.toString().includes('Reject'))
    );
    expect(rejectBtn).toBeDefined();

    await rejectBtn.props.onClick();

    expect(hermesService.gateResolve).toHaveBeenCalledWith('gate-task-02', 'reject', 'Needs revisions.');
  });

});
