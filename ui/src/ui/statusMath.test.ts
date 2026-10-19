import {it,expect} from 'vitest';
import {markGeometry} from './statusMath';
it('morphs dashed idle ring into running arc and complete circle',()=>{
 expect(markGeometry(0,1,80)).toEqual({dash:3,gap:7});expect(markGeometry(1,.75,80)).toEqual({dash:60,gap:20});expect(markGeometry(1,1,80)).toEqual({dash:80,gap:0});
});
