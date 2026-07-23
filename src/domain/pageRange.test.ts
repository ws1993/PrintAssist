import { describe, expect, it } from 'vitest';
import { parsePageRangeExpression } from './pageRange';

describe('parsePageRangeExpression', () => {
  it('parses mixed commas and ranges', () => {
    const result = parsePageRangeExpression('1,3,5-8');
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.pages).toEqual([1, 3, 5, 6, 7, 8]);
    }
  });

  it('rejects inverted ranges', () => {
    const result = parsePageRangeExpression('8-5');
    expect(result.ok).toBe(false);
  });

  it('rejects out of bounds pages', () => {
    const result = parsePageRangeExpression('1,12', 10);
    expect(result.ok).toBe(false);
  });
});
