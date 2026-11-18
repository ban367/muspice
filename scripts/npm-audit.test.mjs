import { describe, expect, it } from 'vitest';
import { ACCEPTED_ADVISORIES, collectAdvisories, judgeAdvisories } from './npm-audit.mjs';

/** `npm audit --json`のアドバイザリ */
function advisory(id, name, severity) {
  return {
    url: `https://github.com/advisories/${id}`,
    name,
    severity,
    title: `${name} issue`,
    range: '<=1.0.0'
  };
}

const report = {
  vulnerabilities: {
    picomatch: { via: [advisory('GHSA-aaaa', 'picomatch', 'high')] },
    // 同じアドバイザリを使っている経路（文字列）は、アドバイザリとして数えない
    tinyglobby: { via: ['picomatch'] },
    fdir: { via: ['picomatch', advisory('GHSA-aaaa', 'picomatch', 'high')] },
    deepmerge: { via: [advisory('GHSA-bbbb', 'deepmerge', 'low')] },
    other: { via: [advisory('GHSA-cccc', 'other', 'moderate')] }
  }
};

describe('collectAdvisories', () => {
  it('経路の文字列を除き、同じアドバイザリを1つにまとめる', () => {
    expect(collectAdvisories(report).map((item) => item.id)).toEqual([
      'GHSA-aaaa',
      'GHSA-bbbb',
      'GHSA-cccc'
    ]);
  });

  it('脆弱性がなければ空', () => {
    expect(collectAdvisories({ vulnerabilities: {} })).toEqual([]);
    expect(collectAdvisories({})).toEqual([]);
  });
});

describe('judgeAdvisories', () => {
  it('受容済みとlowは失敗にせず、受容済みでもう出ていないものを知らせる', () => {
    const accepted = new Map([
      ['GHSA-aaaa', 'reason'],
      ['GHSA-gone', 'reason']
    ]);
    const result = judgeAdvisories(collectAdvisories(report), accepted);

    expect(result.failing.map((item) => item.id)).toEqual(['GHSA-cccc']);
    expect(result.accepted.map((item) => item.id)).toEqual(['GHSA-aaaa']);
    expect(result.stale).toEqual(['GHSA-gone']);
  });

  it('受容済みのものしかなければ、失敗にしない', () => {
    const only = { vulnerabilities: { picomatch: report.vulnerabilities.picomatch } };
    const result = judgeAdvisories(collectAdvisories(only), new Map([['GHSA-aaaa', 'reason']]));
    expect(result.failing).toEqual([]);
  });

  it('受容済みの一覧は、GHSAのIDで書く', () => {
    for (const id of ACCEPTED_ADVISORIES.keys()) {
      expect(id).toMatch(/^GHSA-[0-9a-z]{4}-[0-9a-z]{4}-[0-9a-z]{4}$/);
    }
  });
});
