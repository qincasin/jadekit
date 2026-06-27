import {describe, it, expect} from 'vitest';
import {resolveTabForEvent} from './chatEventRouting';

describe('resolveTabForEvent', () => {
    it('优先按 agentId 命中 tab（覆盖 requestId 映射）', () => {
        const tabs = [
            {key: 't1', agentId: 'a1'},
            {key: 't2', agentId: 'a2'},
        ];
        // requestId 映射指向 t1，但 agentId 指向 t2 —— 应以 agentId 为准。
        const map = new Map([['r9', 't1']]);
        const got = resolveTabForEvent(tabs, {agentId: 'a2', requestId: 'r9'}, map);
        expect(got).toBe('t2');
    });

    it('agentId 缺省时回退 requestId 映射（兼容旧事件）', () => {
        const tabs = [{key: 't1', agentId: 'a1'}];
        const map = new Map([['r9', 't1']]);
        const got = resolveTabForEvent(tabs, {requestId: 'r9'}, map);
        expect(got).toBe('t1');
    });

    it('agentId 与 requestId 都命不中时返回 undefined', () => {
        const tabs = [{key: 't1', agentId: 'a1'}];
        const got = resolveTabForEvent(tabs, {agentId: 'zzz', requestId: 'r9'}, new Map());
        expect(got).toBeUndefined();
    });
});
