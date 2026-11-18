// npmの依存の脆弱性の監査（`.github/workflows/audit.yml`が実行する）
//
// `npm audit`の結果から、受容済みのアドバイザリ（docs/design/non-functional.md の表）を除き、残りに
// moderate以上のものがあれば失敗する。`npm audit`には、アドバイザリを指定して除く仕組みがないため、
// Rustの`cargo-audit --ignore`と同じことをここで行う。表を変えたら、`ACCEPTED_ADVISORIES`も合わせる。
//
// 依存のパッケージを使わない（`npm ci`をせずに、lockfileだけで監査する）。

import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

/** 失敗にする重さ（`npm audit --audit-level=moderate`と同じ） */
const FAILING_SEVERITIES = ['moderate', 'high', 'critical'];

/** 受容済みのアドバイザリ（GHSAのID → 理由。docs/design/non-functional.md の表と同じ） */
export const ACCEPTED_ADVISORIES = new Map([
  [
    'GHSA-fc5q-39xc-vjpf',
    'picomatch: 直した版がない。vite・tinyglobbyなどのビルドの道具が使い、アプリには入らない'
  ]
]);

/**
 * `npm audit --json`の結果から、アドバイザリを取り出す（同じアドバイザリは1つにまとめる）
 *
 * `vulnerabilities`の各パッケージの`via`のうち、オブジェクトがアドバイザリ（文字列は、そのアドバイザリの
 * あるパッケージを使っている、という経路）。
 * @param {{ vulnerabilities?: Record<string, { via: (string | { url: string, name: string, severity: string, title: string, range: string })[] }> }} report
 * @returns {{ id: string, name: string, severity: string, title: string, range: string, url: string }[]}
 */
export function collectAdvisories(report) {
  const advisories = new Map();
  for (const vulnerability of Object.values(report.vulnerabilities ?? {})) {
    for (const via of vulnerability.via) {
      if (typeof via !== 'object' || advisories.has(via.url)) continue;
      advisories.set(via.url, {
        id: via.url.split('/').pop() ?? via.url,
        name: via.name,
        severity: via.severity,
        title: via.title,
        range: via.range,
        url: via.url
      });
    }
  }
  return [...advisories.values()];
}

/**
 * 監査の結果を判定する
 * @param {ReturnType<typeof collectAdvisories>} advisories
 * @param {ReadonlyMap<string, string>} accepted - 受容済みのアドバイザリ
 * @returns {{ failing: typeof advisories, accepted: typeof advisories, stale: string[] }}
 *   失敗にするもの・受容済みで除いたもの・受容済みの一覧にあるが、もう出ていないもの（一覧から外せる）
 */
export function judgeAdvisories(advisories, accepted) {
  const severe = advisories.filter((advisory) => FAILING_SEVERITIES.includes(advisory.severity));
  const found = new Set(advisories.map((advisory) => advisory.id));
  return {
    failing: severe.filter((advisory) => !accepted.has(advisory.id)),
    accepted: severe.filter((advisory) => accepted.has(advisory.id)),
    stale: [...accepted.keys()].filter((id) => !found.has(id))
  };
}

function describe(advisory) {
  return `${advisory.severity} ${advisory.name} ${advisory.range}: ${advisory.title} (${advisory.url})`;
}

function main() {
  const result = spawnSync('npm', ['audit', '--json'], { encoding: 'utf8' });
  if (result.error) throw result.error;
  let report;
  try {
    report = JSON.parse(result.stdout);
  } catch {
    console.error(result.stdout, result.stderr);
    throw new Error('Could not read the output of npm audit');
  }
  if (report.error) {
    throw new Error(`npm audit failed: ${JSON.stringify(report.error)}`);
  }

  const { failing, accepted, stale } = judgeAdvisories(
    collectAdvisories(report),
    ACCEPTED_ADVISORIES
  );
  for (const advisory of accepted) {
    console.log(`Accepted: ${describe(advisory)} - ${ACCEPTED_ADVISORIES.get(advisory.id)}`);
  }
  for (const id of stale) {
    console.log(`::notice::${id} is accepted but no longer reported; remove it from the list`);
  }
  if (failing.length > 0) {
    for (const advisory of failing) console.log(`::error::${describe(advisory)}`);
    process.exit(1);
  }
  console.log('No unaccepted advisories at moderate severity or above');
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main();
}
