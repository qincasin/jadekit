import { describe, expect, it } from 'vitest';
import { canRequestHermesLaunch } from './hermesLaunchReadiness';

describe('canRequestHermesLaunch', () => {
  it('allows a launch request without a selected Git workspace when goal and roster are valid', () => {
    expect(canRequestHermesLaunch({
      goal: 'Implement the requested change',
      rosterCount: 1,
      launching: false,
      workspaceLoading: false,
    })).toBe(true);
  });

  it.each([
    ['the goal is empty', { goal: '   ', rosterCount: 1, launching: false, workspaceLoading: false }],
    ['the roster is empty', { goal: 'Implement the requested change', rosterCount: 0, launching: false, workspaceLoading: false }],
    ['a launch is already in progress', { goal: 'Implement the requested change', rosterCount: 1, launching: true, workspaceLoading: false }],
    ['a project folder is being validated', { goal: 'Implement the requested change', rosterCount: 1, launching: false, workspaceLoading: true }],
  ])('rejects a request when %s', (_reason, input) => {
    expect(canRequestHermesLaunch(input)).toBe(false);
  });
});
