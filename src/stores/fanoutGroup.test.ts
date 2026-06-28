import {describe, expect, it} from 'vitest';
import {fanoutTabsOf} from './fanoutGroup';

describe('fanoutTabsOf', () => {
    it('filters tabs by fan-out group id', () => {
        const tabs = [
            {fanoutGroupId: 'g1'},
            {fanoutGroupId: 'g2'},
            {fanoutGroupId: 'g1'},
            {},
        ];

        expect(fanoutTabsOf(tabs, 'g1')).toHaveLength(2);
    });
});
