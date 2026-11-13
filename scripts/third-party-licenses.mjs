/**
 * アプリに組み込む依存ライブラリの、ライセンスの一覧を作る（ADR-091）
 *
 * - Rust: `cargo metadata`で、配布するOS向けに解決した依存クレートを集める
 * - npm: フロントのバンドルに入ったモジュールのパスから、パッケージを集める（`vite.config.js`のプラグインが渡す）
 *
 * パッケージごとに、名前・版・ライセンスの表記・入手先と、パッケージに入っているライセンスの文
 * （`LICENSE*`・`COPYING*`・`NOTICE*`など）を集める。ライセンスの文が入っていないパッケージは、
 * 表記から選んだライセンスの標準の文（`scripts/spdx/`）で補う。
 *
 * 受け入れるライセンス（`ACCEPTED_LICENSES`）にないものが見つかったら、エラーにする（GPLなど、アプリの
 * ライセンス（MIT）と合わない依存が入った時に、ビルドで気付く）。
 *
 * 外部の道具（cargo-aboutなど）は使わない。`cargo`と、取得済みの依存のファイルだけを読む。
 *
 * 型は`third-party-licenses.d.mts`に書く（Node.jsの型定義（@types/node）を入れていないため、
 * `vite.config.js`から読み込まれるこのファイルそのものは、型チェックの対象にしない）。
 */
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SPDX_DIR = join(ROOT, 'scripts', 'spdx');

/**
 * 受け入れるライセンス（SPDXの識別子）
 *
 * アプリ（MIT）に組み込んで配れる、寛容なライセンスと、ファイル単位のコピーレフト（MPL-2.0）。
 * 足す時は、配る時の条件（表示・ソースの提供など）を満たせるかを確かめる。
 */
export const ACCEPTED_LICENSES = new Set([
  'MIT',
  'MIT-0',
  'Apache-2.0',
  'Apache-2.0 WITH LLVM-exception',
  'BSD-2-Clause',
  'BSD-3-Clause',
  '0BSD',
  'ISC',
  'Zlib',
  'Unicode-3.0',
  'Unicode-DFS-2016',
  'MPL-2.0',
  'CC0-1.0',
  'Unlicense',
  'BSL-1.0'
]);

/**
 * ライセンスを宣言していないパッケージの、ライセンス（パッケージの中のライセンスの文で確かめたもの）
 *
 * キーは`<cargo|npm>:<名前>`。宣言がなく、ここにもないパッケージが見つかったら、エラーにする。
 */
const UNDECLARED_LICENSES = {
  // package.jsonに`license`がない。同梱の`LICENSE`はMIT（Copyright (c) 2024 Hunter Johnston ほか）
  'npm:svelte-toolbelt': 'MIT'
};

/** 「A OR B」から選ぶ順（標準の文で補う時に使う。前にあるものを選ぶ） */
const PREFERRED_LICENSES = ['MIT', 'Apache-2.0', 'BSD-3-Clause', 'BSD-2-Clause', 'Zlib'];

/** ライセンスの文として集めるファイルの名前 */
const LICENSE_FILE_PATTERN = /^(licen[sc]e|copying|unlicense|notice|copyright)/i;

/** ライセンスの文として読むファイルの大きさの上限（これより大きいファイルは、ライセンスの文ではない） */
const MAX_LICENSE_FILE_BYTES = 200 * 1024;

/**
 * ライセンスの表記（SPDXの式）を、式の木にする
 *
 * `/`は`OR`として読む（古いクレートの書き方）。`WITH`は、前後をまとめて1つの識別子にする。
 * @param {string} expression
 * @returns {{ any: unknown[] } | { all: unknown[] } | { id: string }}
 */
export function parseLicenseExpression(expression) {
  const tokens = expression
    .replace(/\//g, ' OR ')
    .replace(/([()])/g, ' $1 ')
    .trim()
    .split(/\s+/)
    .filter(Boolean);
  let position = 0;

  const parseOr = () => {
    const items = [parseAnd()];
    while (tokens[position]?.toUpperCase() === 'OR') {
      position++;
      items.push(parseAnd());
    }
    return items.length === 1 ? items[0] : { any: items };
  };
  const parseAnd = () => {
    const items = [parseTerm()];
    while (tokens[position]?.toUpperCase() === 'AND') {
      position++;
      items.push(parseTerm());
    }
    return items.length === 1 ? items[0] : { all: items };
  };
  const parseTerm = () => {
    const token = tokens[position++];
    if (token === undefined) throw new Error(`Cannot parse the license expression: ${expression}`);
    if (token === '(') {
      const inner = parseOr();
      if (tokens[position++] !== ')') {
        throw new Error(`Cannot parse the license expression: ${expression}`);
      }
      return inner;
    }
    if (tokens[position]?.toUpperCase() === 'WITH') {
      const exception = tokens[position + 1];
      position += 2;
      return { id: `${token} WITH ${exception}` };
    }
    return { id: token };
  };

  const tree = parseOr();
  if (position !== tokens.length) {
    throw new Error(`Cannot parse the license expression: ${expression}`);
  }
  return tree;
}

/**
 * 式の木から、受け入れるライセンスだけで満たせる選び方を返す（満たせなければnull）
 *
 * `OR`は、受け入れられるもののうち、`PREFERRED_LICENSES`で前にあるものを選ぶ。`AND`は、すべてが要る。
 * @returns {string[] | null} 選んだライセンスの識別子
 */
export function chooseLicenses(tree) {
  if ('id' in tree) return ACCEPTED_LICENSES.has(tree.id) ? [tree.id] : null;
  if ('all' in tree) {
    const chosen = tree.all.map(chooseLicenses);
    return chosen.some((ids) => ids === null) ? null : chosen.flat();
  }
  const rank = (ids) => {
    const ranks = ids.map((id) => PREFERRED_LICENSES.indexOf(id));
    return ranks.some((r) => r === -1) ? PREFERRED_LICENSES.length : Math.max(...ranks);
  };
  const candidates = tree.any.map(chooseLicenses).filter((ids) => ids !== null);
  if (candidates.length === 0) return null;
  return candidates.reduce((best, ids) => (rank(ids) < rank(best) ? ids : best));
}

/** フォルダの直下にある、ライセンスの文のファイルを読む（名前順） */
function readLicenseFiles(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory)
    .filter((name) => LICENSE_FILE_PATTERN.test(name))
    .sort()
    .map((name) => join(directory, name))
    .filter((path) => {
      const stats = statSync(path);
      return stats.isFile() && stats.size <= MAX_LICENSE_FILE_BYTES;
    })
    .map((path) => normalizeText(readFileSync(path, 'utf8')))
    .filter((text) => text.length > 0);
}

/** 改行をそろえ、前後の空白を除く（同じ文を1つにまとめるため） */
function normalizeText(text) {
  return text
    .replace(/\r\n?/g, '\n')
    .replace(/[ \t]+$/gm, '')
    .trim();
}

/** ライセンスの標準の文（`scripts/spdx/<識別子>.txt`）。なければnull */
function standardText(id, holders) {
  const path = join(SPDX_DIR, `${id}.txt`);
  if (!existsSync(path)) return null;
  const copyright = `Copyright (c) ${holders}`;
  return normalizeText(readFileSync(path, 'utf8').replace('{{copyright}}', copyright));
}

/**
 * 1つのパッケージの、一覧の項目を作る
 * @param {{ ecosystem: 'cargo' | 'npm', name: string, version: string, license: string | null | undefined,
 *   repository: string | null | undefined, authors: string[], directory: string }} source
 * @returns {{ ecosystem: string, name: string, version: string, license: string, repository: string | null, texts: string[] }}
 */
export function describePackage(source) {
  const label = `${source.name} ${source.version} (${source.ecosystem})`;
  const license = source.license || UNDECLARED_LICENSES[`${source.ecosystem}:${source.name}`];
  if (!license) {
    throw new Error(
      `${label} does not declare a license. Check the license text in the package and add it to ` +
        'UNDECLARED_LICENSES (scripts/third-party-licenses.mjs).'
    );
  }
  const chosen = chooseLicenses(parseLicenseExpression(license));
  if (chosen === null) {
    throw new Error(
      `${label} is licensed under "${license}", which is not in the accepted licenses ` +
        '(scripts/third-party-licenses.mjs). Check that the license allows distributing it with ' +
        'this app before adding it.'
    );
  }

  let texts = readLicenseFiles(source.directory);
  if (texts.length === 0) {
    // ライセンスの文が入っていないパッケージは、選んだライセンスの標準の文で補う
    const holders =
      source.authors.length > 0 ? source.authors.join(', ') : `the ${source.name} authors`;
    texts = chosen.map((id) => {
      const text = standardText(id, holders);
      if (text === null) {
        throw new Error(
          `${label} ships no license file, and scripts/spdx/${id}.txt does not exist. ` +
            'Add the standard text of the license there.'
        );
      }
      return text;
    });
  }
  return {
    ecosystem: source.ecosystem,
    name: source.name,
    version: source.version,
    license,
    repository: source.repository ?? null,
    texts
  };
}

/** `rustc`が動いているOS・CPUの名前（`aarch64-apple-darwin`など） */
function hostTriple() {
  const output = execFileSync('rustc', ['-vV'], { encoding: 'utf8' });
  const host = output.split('\n').find((line) => line.startsWith('host: '));
  if (!host) throw new Error('Cannot find the host triple in the output of `rustc -vV`');
  return host.slice('host: '.length).trim();
}

/**
 * Rustの依存クレートを集める
 *
 * `cargo metadata`を、配布するOS・CPU（Tauriのビルドが渡す`TAURI_ENV_TARGET_TRIPLE`。なければ、
 * 動かしているOS・CPU）向けに絞って呼ぶ。ビルド用の依存・手続きマクロも入る（多めに載せる分には困らない）。
 * @returns {{ target: string, packages: ReturnType<typeof describePackage>[] }}
 */
export function collectRustPackages() {
  const target = process.env.TAURI_ENV_TARGET_TRIPLE || hostTriple();
  const output = execFileSync(
    'cargo',
    [
      'metadata',
      '--format-version',
      '1',
      '--locked',
      '--filter-platform',
      target,
      '--manifest-path',
      join(ROOT, 'src-tauri', 'Cargo.toml')
    ],
    { encoding: 'utf8', maxBuffer: 512 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] }
  );
  const metadata = JSON.parse(output);
  const resolved = new Set(metadata.resolve.nodes.map((node) => node.id));
  const packages = metadata.packages
    // アプリ自身（レジストリ・gitから取っていないもの）は除く
    .filter((pkg) => resolved.has(pkg.id) && pkg.source !== null)
    .map((pkg) =>
      describePackage({
        ecosystem: 'cargo',
        name: pkg.name,
        version: pkg.version,
        license: pkg.license,
        repository: pkg.repository ?? pkg.homepage,
        authors: (pkg.authors ?? []).map((author) => author.replace(/\s*<[^>]*>/, '')),
        directory: dirname(pkg.manifest_path)
      })
    );
  return { target, packages };
}

/** モジュールのパスから、npmのパッケージのフォルダを求める（`node_modules`の外ならnull） */
export function npmPackageDirectory(moduleId) {
  const path = moduleId.split('?')[0].replace(/^\0+/, '');
  const marker = '/node_modules/';
  const index = path.lastIndexOf(marker);
  if (index === -1) return null;
  const rest = path.slice(index + marker.length).split('/');
  const name = rest[0].startsWith('@') ? rest.slice(0, 2).join('/') : rest[0];
  if (!name || name.startsWith('.')) return null;
  return path.slice(0, index + marker.length) + name;
}

/**
 * npmのパッケージを集める
 * @param {Iterable<string>} moduleIds - バンドルに入ったモジュールのパス
 * @param {string[]} extraPackages - モジュールとしては現れないが、成果物に入るパッケージの名前
 *   （CSSに入る`tailwindcss`など）
 */
export function collectNpmPackages(moduleIds, extraPackages = []) {
  const directories = new Set();
  for (const id of moduleIds) {
    const directory = npmPackageDirectory(id);
    if (directory) directories.add(directory);
  }
  for (const name of extraPackages) {
    directories.add(join(ROOT, 'node_modules', name));
  }
  return [...directories]
    .filter((directory) => existsSync(join(directory, 'package.json')))
    .map((directory) => {
      const pkg = JSON.parse(readFileSync(join(directory, 'package.json'), 'utf8'));
      const repository = typeof pkg.repository === 'string' ? pkg.repository : pkg.repository?.url;
      const author = typeof pkg.author === 'string' ? pkg.author : pkg.author?.name;
      return describePackage({
        ecosystem: 'npm',
        name: pkg.name,
        version: pkg.version,
        license: typeof pkg.license === 'string' ? pkg.license : pkg.license?.type,
        repository: repository ?? pkg.homepage,
        authors: author ? [author.replace(/\s*<[^>]*>/, '')] : [],
        directory
      });
    });
}

/**
 * 一覧を、アプリに埋め込む形にする
 *
 * 同じライセンスの文は1つにまとめ、パッケージからは番号で指す（Apache-2.0の文などは、多くの
 * パッケージで同じ）。名前・版の順に並べ、同じ入力からは同じ結果になるようにする。
 * @param {{ target: string | null, packages: ReturnType<typeof describePackage>[], rustMissing?: boolean }} input
 */
export function buildLicenseList({ target, packages, rustMissing = false }) {
  const texts = [];
  const indexByHash = new Map();
  const unique = new Map();
  for (const pkg of packages) {
    unique.set(`${pkg.ecosystem}:${pkg.name}@${pkg.version}`, pkg);
  }
  const sorted = [...unique.values()].sort(
    (a, b) =>
      a.name.toLowerCase().localeCompare(b.name.toLowerCase()) ||
      a.version.localeCompare(b.version) ||
      a.ecosystem.localeCompare(b.ecosystem)
  );
  return {
    target,
    rustMissing,
    packages: sorted.map((pkg) => ({
      ecosystem: pkg.ecosystem,
      name: pkg.name,
      version: pkg.version,
      license: pkg.license,
      repository: pkg.repository,
      texts: pkg.texts.map((text) => {
        const hash = createHash('sha256').update(text).digest('hex');
        let index = indexByHash.get(hash);
        if (index === undefined) {
          index = texts.push(text) - 1;
          indexByHash.set(hash, index);
        }
        return index;
      })
    })),
    texts
  };
}
