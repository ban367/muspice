import { describe, expect, it } from 'vitest';
import { EMPTY_ID_SET, idSetOf, indexById, mapById, setOf } from './collections.js';

describe('collections', () => {
  const items = [
    { id: 'a', name: 'A' },
    { id: 'b', name: 'B' }
  ];

  it('値・IDの集まりを作る', () => {
    expect([...setOf(['x', 'y', 'x'])]).toEqual(['x', 'y']);
    expect(idSetOf(items).has('b')).toBe(true);
    expect(EMPTY_ID_SET.size).toBe(0);
  });

  it('IDから項目・位置を引く', () => {
    expect(mapById(items).get('b')?.name).toBe('B');
    expect(indexById(items).get('b')).toBe(1);
    expect(indexById(items).get('z')).toBeUndefined();
  });
});
