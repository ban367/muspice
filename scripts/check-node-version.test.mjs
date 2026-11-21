import { describe, expect, it } from 'vitest';
import {
  compareVersions,
  findNodeUpdate,
  findOpenIssue,
  issueBody,
  issueTitle,
  parseVersion
} from './check-node-version.mjs';

/** nodejs.orgの版の一覧の1件 */
function release(version, { security = false, lts = false, date = '2026-11-10' } = {}) {
  return { version, date, security, lts };
}

// 実際の一覧と同じく新しい順だが、正式でない版も混じる
const releases = [
  release('v27.0.0-alpha.0.1.1'),
  release('v26.13.1', { lts: 'Lithium' }),
  release('v25.9.0'),
  release('v24.22.3', { security: true, lts: 'Krypton' }),
  release('v24.22.2', { lts: 'Krypton', date: '2026-10-27' }),
  release('v24.22.1', { security: true, lts: 'Krypton', date: '2026-10-20' }),
  release('v22.30.0', { lts: 'Jod' })
];

describe('parseVersion', () => {
  it('先頭のvと前後の空白を除いて、数の組にする', () => {
    expect(parseVersion('v24.22.3')).toEqual([24, 22, 3]);
    expect(parseVersion('24.22.2\n')).toEqual([24, 22, 2]);
  });

  it('正式な版でなければnull', () => {
    expect(parseVersion('v27.0.0-alpha.0.1.1')).toBeNull();
    expect(parseVersion('24')).toBeNull();
    expect(parseVersion('lts/*')).toBeNull();
  });
});

describe('compareVersions', () => {
  it('メジャー・マイナー・パッチの順に比べる', () => {
    expect(compareVersions([24, 22, 3], [24, 22, 2])).toBeGreaterThan(0);
    expect(compareVersions([24, 9, 0], [24, 10, 0])).toBeLessThan(0);
    expect(compareVersions([26, 0, 0], [24, 99, 99])).toBeGreaterThan(0);
    expect(compareVersions([24, 22, 3], [24, 22, 3])).toBe(0);
  });
});

describe('findNodeUpdate', () => {
  it('同じメジャーの新しい版と、セキュリティリリースを含むかを返す', () => {
    const update = findNodeUpdate('24.22.2', releases);
    expect(update.newer.map((item) => item.version)).toEqual(['v24.22.3']);
    expect(update.latest?.version).toBe('v24.22.3');
    expect(update.security).toBe(true);
  });

  it('いくつか遅れていれば、新しい順に並べる', () => {
    const update = findNodeUpdate('24.22.1', [...releases].reverse());
    expect(update.newer.map((item) => item.version)).toEqual(['v24.22.3', 'v24.22.2']);
    expect(update.latest?.version).toBe('v24.22.3');
  });

  it('セキュリティリリースでない版だけなら、securityはfalse', () => {
    const update = findNodeUpdate('24.22.1', [release('v24.22.2'), release('v24.22.1')]);
    expect(update.latest?.version).toBe('v24.22.2');
    expect(update.security).toBe(false);
  });

  it('最新なら、latestはnull', () => {
    const update = findNodeUpdate('24.22.3', releases);
    expect(update.newer).toEqual([]);
    expect(update.latest).toBeNull();
    expect(update.security).toBe(false);
  });

  it('より新しいメジャーのLTSを返す（LTSでないメジャー・正式でない版は除く）', () => {
    expect(findNodeUpdate('24.22.3', releases).newerLts?.version).toBe('v26.13.1');
    expect(findNodeUpdate('26.13.1', releases).newerLts).toBeNull();
  });

  it('.node-versionの版が正式な版でなければ失敗する', () => {
    expect(() => findNodeUpdate('24', releases)).toThrow('Invalid Node.js version');
  });
});

describe('findOpenIssue', () => {
  const issues = [
    { number: 1, title: 'Node.js 26 LTS への更新' },
    { number: 2, title: 'Node.js を 22.30.0 に上げる' },
    { number: 3, title: 'Node.js を 24.22.3 に上げる' }
  ];

  it('同じメジャーの版を上げるIssueを返す', () => {
    expect(findOpenIssue(issues, 24)?.number).toBe(3);
    expect(findOpenIssue(issues, 22)?.number).toBe(2);
  });

  it('なければnull', () => {
    expect(findOpenIssue(issues, 26)).toBeNull();
    expect(findOpenIssue([], 24)).toBeNull();
  });
});

describe('issueTitle / issueBody', () => {
  it('題名は、上げる先の版（vを除く）', () => {
    expect(issueTitle('v24.22.3')).toBe('Node.js を 24.22.3 に上げる');
  });

  it('本文に、今の版・新しい版・セキュリティリリース・新しいLTSを書く', () => {
    const body = issueBody('24.22.1', findNodeUpdate('24.22.1', releases));
    expect(body).toContain('`.node-version`（24.22.1）より新しい Node.js 24.22.3 が出ています。');
    expect(body).toContain('セキュリティリリースを含みます。');
    expect(body).toContain('| 24.22.3 | 2026-11-10 | はい |');
    expect(body).toContain('| 24.22.2 | 2026-10-27 | いいえ |');
    expect(body).toContain('CHANGELOG_V24.md');
    expect(body).toContain('- [ ] `.node-version` を 24.22.3 にする');
    expect(body).toContain('Node.js 26.13.1（Lithium）は、より新しいLTSです。');
  });

  it('セキュリティリリースでなく、新しいLTSもなければ、その文は書かない', () => {
    const body = issueBody(
      '24.22.1',
      findNodeUpdate('24.22.1', [release('v24.22.2'), release('v24.22.1')])
    );
    expect(body).not.toContain('セキュリティリリースを含みます。');
    expect(body).not.toContain('より新しいLTS');
  });

  it('新しい版がなければ失敗する', () => {
    expect(() => issueBody('24.22.3', findNodeUpdate('24.22.3', releases))).toThrow();
  });
});
