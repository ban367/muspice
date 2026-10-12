import { describe, expect, it } from 'vitest';
import { hasValue, listedArtists, splitValues } from './multiValue.js';

describe('splitValues', () => {
  it('`;`で区切って、前後の空白を除く', () => {
    expect(splitValues('Rock; Pop')).toEqual(['Rock', 'Pop']);
    expect(splitValues('Rock;Pop ;  Jazz')).toEqual(['Rock', 'Pop', 'Jazz']);
  });

  it('`/`・`,`・`&`では分けない', () => {
    expect(splitValues('AC/DC')).toEqual(['AC/DC']);
    expect(splitValues('Earth, Wind & Fire; Chicago')).toEqual(['Earth, Wind & Fire', 'Chicago']);
  });

  it('空の値と、同じ値の2つ目以降は除く', () => {
    expect(splitValues('Rock;; Rock ; ')).toEqual(['Rock']);
    expect(splitValues(' ; ')).toEqual([]);
    expect(splitValues('')).toEqual([]);
    expect(splitValues(null)).toEqual([]);
    expect(splitValues(undefined)).toEqual([]);
  });
});

describe('hasValue', () => {
  it('値のどれかと、完全に一致するかを調べる', () => {
    expect(hasValue('Rock; Pop', 'Pop')).toBe(true);
    expect(hasValue('Rock; Pop', 'Po')).toBe(false);
    expect(hasValue('Rock; Pop', 'Rock; Pop')).toBe(false);
    expect(hasValue(null, 'Pop')).toBe(false);
  });
});

describe('listedArtists', () => {
  it('アルバムアーティストがあれば、それだけ', () => {
    expect(listedArtists({ artist: 'A; B', albumArtist: 'Various Artists' })).toEqual([
      'Various Artists'
    ]);
  });

  it('なければ、曲のアーティストのそれぞれの値', () => {
    expect(listedArtists({ artist: 'A; B', albumArtist: null })).toEqual(['A', 'B']);
    expect(listedArtists({ artist: 'AC/DC', albumArtist: null })).toEqual(['AC/DC']);
    expect(listedArtists({ artist: null, albumArtist: null })).toEqual([]);
  });
});
