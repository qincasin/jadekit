import { describe, expect, it } from 'vitest';
import {
  extractHermesLaunchErrorReason,
  getHermesLaunchErrorMessageKey,
  isHermesRunStillActiveError,
} from './hermesLaunchError';

describe('extractHermesLaunchErrorReason', () => {
  it('normalizes string rejection reasons', () => {
    expect(extractHermesLaunchErrorReason('  Dispatch\n\tfailed  ')).toBe('Dispatch failed');
  });

  it('extracts an Error message', () => {
    expect(extractHermesLaunchErrorReason(new Error('Hermes is unavailable'))).toBe('Hermes is unavailable');
  });

  it('preserves the reason for an Error without a message', () => {
    expect(extractHermesLaunchErrorReason(new Error())).toBe('Error');
  });

  it('extracts an object rejection message', () => {
    expect(extractHermesLaunchErrorReason({ message: '  Backend rejected request  ' }))
      .toBe('Backend rejected request');
  });

  it('extracts only a nested error message from object rejection reasons', () => {
    expect(extractHermesLaunchErrorReason({
      error: { message: '  Repository status unavailable  ', code: 'INTERNAL' },
    })).toBe('Repository status unavailable');
  });

  it('does not expose arbitrary object rejection fields', () => {
    expect(extractHermesLaunchErrorReason({
      code: 'INTERNAL',
      stack: 'sensitive stack trace',
      token: 'secret-token',
    })).toBe('');
  });

  it('safely truncates unknown rejection reasons', () => {
    expect(extractHermesLaunchErrorReason('x'.repeat(501))).toBe(`${'x'.repeat(500)}...`);
  });
});

describe('isHermesRunStillActiveError', () => {
  it('recognizes the active Hermes run rejection', () => {
    expect(isHermesRunStillActiveError(
      'Cannot start a new Hermes run while run hermes-123 is still active'
    )).toBe(true);
  });

  it('does not classify unrelated errors as an active run', () => {
    expect(isHermesRunStillActiveError('Hermes backend is unavailable')).toBe(false);
  });
});

describe('getHermesLaunchErrorMessageKey', () => {
  it('uses a generic message when the sanitized reason is empty', () => {
    expect(getHermesLaunchErrorMessageKey('')).toBe('launchErrorGeneric');
  });

  it('uses the reason-bearing message when a sanitized reason is available', () => {
    expect(getHermesLaunchErrorMessageKey('Hermes backend is unavailable')).toBe('launchErrorUnknown');
  });

  it('prioritizes the active-run message over the generic error message', () => {
    expect(getHermesLaunchErrorMessageKey(
      'Cannot start a new Hermes run while run hermes-123 is still active'
    )).toBe('launchErrorActiveRun');
  });
});
