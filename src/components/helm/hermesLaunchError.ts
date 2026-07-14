const MAX_ERROR_REASON_LENGTH = 500;

export type HermesLaunchErrorMessageKey =
  | 'launchErrorActiveRun'
  | 'launchErrorGeneric'
  | 'launchErrorUnknown';

const normalizeReason = (value: string): string => {
  const cleaned = value.replace(/[\u0000-\u001F\u007F]+/g, ' ').replace(/\s+/g, ' ').trim();
  return cleaned.length > MAX_ERROR_REASON_LENGTH
    ? `${cleaned.slice(0, MAX_ERROR_REASON_LENGTH)}...`
    : cleaned;
};

const extractObjectReason = (error: object): string => {
  try {
    const candidate = error as Record<string, unknown>;
    for (const key of ['message', 'reason', 'detail']) {
      const value = candidate[key];
      if (typeof value === 'string') return normalizeReason(value);
    }

    const nestedError = candidate.error;
    if (nestedError && typeof nestedError === 'object') {
      const message = (nestedError as Record<string, unknown>).message;
      if (typeof message === 'string') return normalizeReason(message);
    }
  } catch {
    // Rejected values may be proxies or have throwing accessors.
  }

  return '';
};

export const extractHermesLaunchErrorReason = (error: unknown): string => {
  if (typeof error === 'string') return normalizeReason(error);
  if (error instanceof Error) return normalizeReason(error.message || String(error));

  if (error && typeof error === 'object') {
    return extractObjectReason(error);
  }

  return normalizeReason(String(error));
};

export const isHermesRunStillActiveError = (reason: string): boolean =>
  /^Cannot start a new Hermes run while run .+ is still active$/i.test(reason);

export const getHermesLaunchErrorMessageKey = (reason: string): HermesLaunchErrorMessageKey => {
  if (isHermesRunStillActiveError(reason)) return 'launchErrorActiveRun';
  return reason ? 'launchErrorUnknown' : 'launchErrorGeneric';
};
