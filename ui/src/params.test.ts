import { expect, it } from 'vitest';
import { parseParams } from './params';
it('requires an object and rejects unsafe integer parameters before invoke', () => {
  expect(parseParams('{"id":"900719925474099312345","nested":[1,null,true]}')).toEqual({ id: '900719925474099312345', nested: [1, null, true] });
  expect(() => parseParams('[]')).toThrow('JSON 객체');
  expect(() => parseParams('{"n":9007199254740993}')).toThrow('안전하지 않은 숫자');
  expect(() => parseParams('{"n":1e999}')).toThrow('안전하지 않은 숫자');
  expect(() => parseParams('{')).toThrow();
});
