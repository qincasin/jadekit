import { vi, describe, it, expect, beforeEach } from 'vitest';

// Define the mock helper inside/outside of vi.mock.
// Variables referenced in vi.mock must start with "mock".
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
      // Run effects when state updates
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

vi.mock('react', () => {
  return {
    useState: (init: any) => mockReactHarness.useState(init),
    useEffect: (eff: any, deps: any) => mockReactHarness.useEffect(eff, deps),
  };
});

// Setup mock localStorage
const mockLocalStorageStore: Record<string, string> = {};
const mockLocalStorage = {
  getItem: vi.fn((key: string) => mockLocalStorageStore[key] || null),
  setItem: vi.fn((key: string, value: string) => {
    mockLocalStorageStore[key] = String(value);
  }),
  removeItem: vi.fn((key: string) => {
    delete mockLocalStorageStore[key];
  }),
  clear: vi.fn(() => {
    for (const key in mockLocalStorageStore) {
      delete mockLocalStorageStore[key];
    }
  }),
  length: 0,
  key: vi.fn((_index: number) => null),
};

Object.defineProperty(globalThis, 'localStorage', {
  value: mockLocalStorage,
  writable: true,
});

// Import the hook to test
import { useCockpitLayout } from './useCockpitLayout';

describe('useCockpitLayout', () => {
  beforeEach(() => {
    mockReactHarness.reset();
    localStorage.clear();
    vi.clearAllMocks();
  });

  it('should default to open for both left and right panes', () => {
    mockReactHarness.resetIndex();
    const result = useCockpitLayout();

    expect(result.leftOpen).toBe(true);
    expect(result.rightOpen).toBe(true);
    expect(localStorage.setItem).toHaveBeenCalledWith('helm-cockpit-left-open', 'true');
    expect(localStorage.setItem).toHaveBeenCalledWith('helm-cockpit-right-open', 'true');
  });

  it('should restore states from localStorage', () => {
    localStorage.setItem('helm-cockpit-left-open', 'false');
    localStorage.setItem('helm-cockpit-right-open', 'false');
    vi.clearAllMocks();

    mockReactHarness.resetIndex();
    const result = useCockpitLayout();

    expect(result.leftOpen).toBe(false);
    expect(result.rightOpen).toBe(false);
  });

  it('should toggle left pane and persist to localStorage', () => {
    mockReactHarness.resetIndex();
    let result = useCockpitLayout();
    expect(result.leftOpen).toBe(true);

    // Toggle left pane
    result.toggleLeft();

    // Re-run hook to get updated state (simulating React re-render)
    mockReactHarness.resetIndex();
    result = useCockpitLayout();

    expect(result.leftOpen).toBe(false);
    expect(localStorage.setItem).toHaveBeenCalledWith('helm-cockpit-left-open', 'false');
  });

  it('should toggle right pane and persist to localStorage', () => {
    mockReactHarness.resetIndex();
    let result = useCockpitLayout();
    expect(result.rightOpen).toBe(true);

    // Toggle right pane
    result.toggleRight();

    // Re-run hook to get updated state
    mockReactHarness.resetIndex();
    result = useCockpitLayout();

    expect(result.rightOpen).toBe(false);
    expect(localStorage.setItem).toHaveBeenCalledWith('helm-cockpit-right-open', 'false');
  });
});
