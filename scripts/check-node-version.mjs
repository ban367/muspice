// Node.jsの版の確認（`.github/workflows/node-version.yml`が実行する）
//
// `.node-version`の版を、nodejs.orgの版の一覧の、同じメジャーの最新の版と比べる。Dependabotは
// `.node-version`を上げないため、ここで遅れに気づけるようにする（ADR-124）。
// 遅れていれば、`--issue`を付けた時にIssueを立てる（同じメジャーのIssueが開いていれば、版を書き換える）。
// PRにしないのは、`GITHUB_TOKEN`で作ったPRではCIが走らないため。
//
// 依存のパッケージを使わない（`npm ci`をせずに動かす）。

import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const INDEX_URL = 'https://nodejs.org/dist/index.json';

/**
 * nodejs.orgの版の一覧（`index.json`）の1件
 * @typedef {{ version: string, date: string, security: boolean, lts: string | false }} Release
 */

/**
 * 版の文字列を数の組にする（先頭の`v`は除く）。正式な版（`24.22.3`の形）でなければnull
 * @param {string} text
 * @returns {[number, number, number] | null}
 */
export function parseVersion(text) {
  const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(text.trim());
  return match ? [Number(match[1]), Number(match[2]), Number(match[3])] : null;
}

/**
 * 版を比べる
 * @param {[number, number, number]} a
 * @param {[number, number, number]} b
 * @returns {number} aがbより新しければ正、古ければ負、同じなら0
 */
export function compareVersions(a, b) {
  for (let i = 0; i < 3; i++) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return 0;
}

/**
 * 今の版より新しい、同じメジャーの版を探す
 *
 * 正式な版でないもの（`27.0.0-alpha`など）は除く。
 * @param {string} current - `.node-version`の版
 * @param {Release[]} releases - nodejs.orgの版の一覧
 * @returns {{ newer: Release[], latest: Release | null, security: boolean, newerLts: Release | null }}
 *   同じメジャーの新しい版（新しい順）・その最新・セキュリティリリースを含むか・
 *   今より新しいメジャーのLTSの最新（メジャーの更新は、このスクリプトでは扱わず、知らせるだけ）
 */
export function findNodeUpdate(current, releases) {
  const base = parseVersion(current);
  if (!base) throw new Error(`Invalid Node.js version in .node-version: ${current}`);
  const parsed = releases
    .map((release) => ({ release, version: parseVersion(release.version) }))
    .filter((item) => item.version !== null)
    .sort((a, b) => compareVersions(b.version, a.version));
  const newer = parsed
    .filter(({ version }) => version[0] === base[0] && compareVersions(version, base) > 0)
    .map(({ release }) => release);
  const newerLts =
    parsed.find(({ release, version }) => version[0] > base[0] && release.lts !== false)?.release ??
    null;
  return {
    newer,
    latest: newer[0] ?? null,
    security: newer.some((release) => release.security),
    newerLts
  };
}

/** 版から`v`を除く */
function bare(version) {
  return version.replace(/^v/, '');
}

/**
 * Issueの題名
 * @param {string} version - 上げる先の版
 */
export function issueTitle(version) {
  return `Node.js を ${bare(version)} に上げる`;
}

/**
 * 開いているIssueから、同じメジャーの版を上げるIssue（このスクリプトが立てたもの）を探す
 * @param {{ number: number, title: string }[]} issues
 * @param {number} major
 */
export function findOpenIssue(issues, major) {
  const pattern = new RegExp(`^Node\\.js を ${major}\\.\\d+\\.\\d+ に上げる$`);
  return issues.find((issue) => pattern.test(issue.title)) ?? null;
}

/**
 * Issueの本文
 * @param {string} current - `.node-version`の版
 * @param {ReturnType<typeof findNodeUpdate>} update
 */
export function issueBody(current, update) {
  if (!update.latest) throw new Error('No newer Node.js version');
  const major = parseVersion(update.latest.version)?.[0];
  const rows = update.newer.map(
    (release) =>
      `| ${bare(release.version)} | ${release.date} | ${release.security ? 'はい' : 'いいえ'} |`
  );
  const lines = [
    '## 概要',
    '',
    `\`.node-version\`（${bare(current)}）より新しい Node.js ${bare(update.latest.version)} が出ています。` +
      (update.security ? 'セキュリティリリースを含みます。' : ''),
    '',
    'このIssueは、`.github/workflows/node-version.yml` が毎週の確認で立てたものです（ADR-124）。',
    '',
    '## 新しい版',
    '',
    '| 版 | 日付 | セキュリティリリース |',
    '| --- | --- | --- |',
    ...rows,
    '',
    `変更点: https://github.com/nodejs/node/blob/main/doc/changelogs/CHANGELOG_V${major}.md`,
    '',
    '## 作業内容',
    '',
    `- [ ] \`.node-version\` を ${bare(update.latest.version)} にする`,
    '- [ ] 同梱のnpmの版が変わる時は、lockfileへの影響を確かめる（`docs/design/implementation.md` の「ツールチェーンの版」）',
    '- [ ] `README.md`・`docs/design/implementation.md` の版の記載を直す'
  ];
  if (update.newerLts) {
    lines.push(
      '',
      `Node.js ${bare(update.newerLts.version)}（${update.newerLts.lts}）は、より新しいLTSです。メジャーの更新は、別のIssueで扱います。`
    );
  }
  return lines.join('\n');
}

/** ghコマンドを実行して、標準出力を返す */
function gh(args) {
  const result = spawnSync('gh', args, { encoding: 'utf8' });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`gh ${args.slice(0, 2).join(' ')} failed: ${result.stderr}`);
  }
  return result.stdout;
}

/** 遅れを知らせるIssueを立てる（同じメジャーのIssueが開いていれば、題名と本文を書き換える） */
function reportIssue(current, update) {
  if (!update.latest) return;
  const title = issueTitle(update.latest.version);
  const body = issueBody(current, update);
  const issues = JSON.parse(
    gh(['issue', 'list', '--state', 'open', '--limit', '200', '--json', 'number,title'])
  );
  const existing = findOpenIssue(issues, parseVersion(update.latest.version)?.[0] ?? -1);
  if (existing?.title === title) {
    console.log(`Issue #${existing.number} already tracks Node.js ${bare(update.latest.version)}`);
    return;
  }
  if (existing) {
    gh(['issue', 'edit', String(existing.number), '--title', title, '--body', body]);
    console.log(`Updated issue #${existing.number}: ${title}`);
    return;
  }
  const args = ['issue', 'create', '--title', title, '--body', body, '--label', 'dependencies'];
  if (process.env.ISSUE_ASSIGNEE) args.push('--assignee', process.env.ISSUE_ASSIGNEE);
  console.log(`Created issue: ${gh(args).trim()}`);
}

async function main() {
  const current = readFileSync('.node-version', 'utf8').trim();
  const response = await fetch(INDEX_URL);
  if (!response.ok) throw new Error(`Could not fetch ${INDEX_URL}: ${response.status}`);
  const update = findNodeUpdate(current, await response.json());

  if (update.newerLts) {
    console.log(
      `::notice::Node.js ${bare(update.newerLts.version)} (${update.newerLts.lts}) is a newer LTS major line`
    );
  }
  if (!update.latest) {
    console.log(`Node.js ${current} is the latest release in its major line`);
    return;
  }
  const security = update.security ? ', including a security release' : '';
  console.log(
    `::warning::Node.js ${bare(update.latest.version)} is available (current: ${current}${security})`
  );
  if (process.argv.includes('--issue')) reportIssue(current, update);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  await main();
}
