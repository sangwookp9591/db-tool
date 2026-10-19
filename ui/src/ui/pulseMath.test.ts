import {it,expect} from 'vitest';
import {pulseSwell} from './pulseMath';
it('contracts to a dot, then overshoots and returns without idle work',()=>{
 expect(pulseSwell(0)).toBe(0);expect(pulseSwell(.4)).toBeCloseTo(1);expect(pulseSwell(.8)).toBeLessThan(0);expect(pulseSwell(1)).toBe(0);
});
