import { describe, it, expect } from 'vitest';
import { cleanTitle, titleHashtags } from './reels';

describe('link titles', () => {
  const fb = '747K views · 6.4K reactions | قال رجل حكيم ذات مرة: #الارهاب_والكباب #روتانا_كلاسيك | Rotana Classic';

  it('cleans a Facebook reel title', () => {
    expect(cleanTitle(fb, 'Rotana Classic')).toBe('قال رجل حكيم ذات مرة');
  });

  it('turns hashtags into tags', () => {
    expect(titleHashtags(fb)).toEqual(['الارهاب والكباب', 'روتانا كلاسيك']);
    expect(titleHashtags('Goal #football #Football #x')).toEqual(['football']);
  });

  it('leaves ordinary titles alone', () => {
    expect(cleanTitle('غزل البنات (1949)', 'Someone')).toBe('غزل البنات (1949)');
    expect(cleanTitle('#news', null)).toBe('#news');
  });
});
