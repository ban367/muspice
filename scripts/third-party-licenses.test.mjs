import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  buildLicenseList,
  chooseLicenses,
  collectNpmPackages,
  describePackage,
  npmPackageDirectory,
  parseLicenseExpression
} from './third-party-licenses.mjs';

let directory;

beforeEach(() => {
  directory = mkdtempSync(join(tmpdir(), 'muspice-licenses-'));
});

afterEach(() => {
  rmSync(directory, { recursive: true, force: true });
});

/** 確認用のパッケージ（ライセンスの文のファイルは、名前 → 中身） */
function source(overrides = {}, files = {}) {
  const dir = join(directory, overrides.name ?? 'pkg');
  mkdirSync(dir, { recursive: true });
  for (const [name, content] of Object.entries(files)) {
    writeFileSync(join(dir, name), content);
  }
  return {
    ecosystem: 'cargo',
    name: 'pkg',
    version: '1.0.0',
    license: 'MIT',
    repository: 'https://example.com/pkg',
    authors: ['Jane Doe'],
    directory: dir,
    ...overrides
  };
}

const choose = (expression) => chooseLicenses(parseLicenseExpression(expression));

describe('parseLicenseExpression / chooseLicenses', () => {
  it('ORは、受け入れられるもののうち、優先するものを選ぶ', () => {
    expect(choose('MIT OR Apache-2.0')).toEqual(['MIT']);
    expect(choose('Apache-2.0 OR MIT')).toEqual(['MIT']);
    expect(choose('Zlib OR Apache-2.0 OR MIT')).toEqual(['MIT']);
    expect(choose('GPL-3.0 OR MIT')).toEqual(['MIT']);
    expect(choose('0BSD OR Apache-2.0')).toEqual(['Apache-2.0']);
  });

  it('古い書き方の「/」は、ORとして読む', () => {
    expect(choose('MIT/Apache-2.0')).toEqual(['MIT']);
    expect(choose('Apache-2.0 / MIT')).toEqual(['MIT']);
  });

  it('ANDは、すべてが要る', () => {
    expect(choose('(MIT OR Apache-2.0) AND Unicode-3.0')).toEqual(['MIT', 'Unicode-3.0']);
    expect(choose('BSD-3-Clause AND MIT')).toEqual(['BSD-3-Clause', 'MIT']);
    expect(choose('MIT AND GPL-3.0')).toBeNull();
  });

  it('WITHは、前後をまとめて1つのライセンスにする', () => {
    expect(choose('Apache-2.0 WITH LLVM-exception')).toEqual(['Apache-2.0 WITH LLVM-exception']);
    expect(choose('Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT')).toEqual(['MIT']);
    expect(choose('GPL-2.0 WITH Classpath-exception-2.0')).toBeNull();
  });

  it('受け入れないライセンスだけなら、選べない', () => {
    expect(choose('GPL-3.0')).toBeNull();
    expect(choose('LGPL-2.1 OR GPL-3.0')).toBeNull();
  });

  it('読めない式は、エラーにする', () => {
    expect(() => parseLicenseExpression('(MIT OR')).toThrow(/Cannot parse/);
    expect(() => parseLicenseExpression('MIT Apache-2.0')).toThrow(/Cannot parse/);
  });
});

describe('describePackage', () => {
  it('パッケージに入っているライセンスの文を集める', () => {
    const described = describePackage(
      source(
        { license: 'MIT OR Apache-2.0' },
        {
          'LICENSE-MIT': 'MIT text\r\n',
          'LICENSE-APACHE': 'Apache text   \n',
          'README.md': 'not a license',
          'COPYING.md': 'copying'
        }
      )
    );

    expect(described.license).toBe('MIT OR Apache-2.0');
    expect(described.repository).toBe('https://example.com/pkg');
    // 名前順。改行と行末の空白はそろえる
    expect(described.texts).toEqual(['copying', 'Apache text', 'MIT text']);
  });

  it('ライセンスの文がなければ、選んだライセンスの標準の文に、作者の名前を入れて補う', () => {
    const described = describePackage(source({ license: 'Zlib OR Apache-2.0 OR MIT' }));

    expect(described.texts).toHaveLength(1);
    expect(described.texts[0]).toMatch(/^MIT License\n\nCopyright \(c\) Jane Doe\n/);
  });

  it('作者が分からなければ、パッケージの名前で補う', () => {
    const described = describePackage(source({ license: 'BSD-3-Clause', authors: [] }));

    expect(described.texts[0]).toContain('Copyright (c) the pkg authors');
  });

  it('受け入れないライセンスは、エラーにする', () => {
    expect(() => describePackage(source({ name: 'viral', license: 'GPL-3.0-only' }))).toThrow(
      /viral 1\.0\.0 \(cargo\) is licensed under "GPL-3\.0-only"/
    );
  });

  it('ライセンスの宣言がなければ、エラーにする（確かめてあるものは、その内容を使う）', () => {
    expect(() => describePackage(source({ license: null }))).toThrow(/does not declare a license/);

    const known = describePackage(
      source({ ecosystem: 'npm', name: 'svelte-toolbelt', license: undefined }, { LICENSE: 'MIT' })
    );
    expect(known.license).toBe('MIT');
  });

  it('標準の文を持っていないライセンスで、文もなければ、エラーにする', () => {
    expect(() => describePackage(source({ license: 'Unicode-3.0' }))).toThrow(
      /scripts\/spdx\/Unicode-3\.0\.txt does not exist/
    );
  });
});

describe('npmPackageDirectory', () => {
  it('モジュールのパスから、パッケージのフォルダを求める', () => {
    expect(npmPackageDirectory('/app/node_modules/svelte/src/index.js')).toBe(
      '/app/node_modules/svelte'
    );
    expect(npmPackageDirectory('/app/node_modules/@tanstack/query-core/build/index.js?v=1')).toBe(
      '/app/node_modules/@tanstack/query-core'
    );
    // 入れ子の依存は、内側のパッケージ
    expect(npmPackageDirectory('/app/node_modules/bits-ui/node_modules/runed/dist/index.js')).toBe(
      '/app/node_modules/bits-ui/node_modules/runed'
    );
  });

  it('アプリ自身のモジュール・仮想のモジュールは、パッケージではない', () => {
    expect(npmPackageDirectory('/app/src/lib/stores/player.svelte.ts')).toBeNull();
    expect(npmPackageDirectory('\0virtual:module')).toBeNull();
  });
});

describe('collectNpmPackages', () => {
  it('バンドルに入ったパッケージを、package.jsonとライセンスの文から集める', () => {
    const pkg = join(directory, 'node_modules', '@scope', 'lib');
    mkdirSync(join(pkg, 'dist'), { recursive: true });
    writeFileSync(
      join(pkg, 'package.json'),
      JSON.stringify({
        name: '@scope/lib',
        version: '2.3.4',
        license: 'ISC',
        author: { name: 'Lib Author' },
        repository: { type: 'git', url: 'git+https://example.com/lib.git' }
      })
    );
    writeFileSync(join(pkg, 'LICENSE'), 'ISC text');

    const packages = collectNpmPackages([
      join(pkg, 'dist', 'index.js'),
      join(pkg, 'dist', 'other.js'),
      join(directory, 'src', 'app.ts')
    ]);

    expect(packages).toEqual([
      {
        ecosystem: 'npm',
        name: '@scope/lib',
        version: '2.3.4',
        license: 'ISC',
        repository: 'git+https://example.com/lib.git',
        texts: ['ISC text']
      }
    ]);
  });
});

describe('buildLicenseList', () => {
  it('同じ文を1つにまとめ、名前の順に並べる', () => {
    const list = buildLicenseList({
      target: 'aarch64-apple-darwin',
      packages: [
        {
          ecosystem: 'cargo',
          name: 'zeta',
          version: '1.0.0',
          license: 'MIT',
          repository: null,
          texts: ['shared', 'zeta only']
        },
        {
          ecosystem: 'npm',
          name: 'Alpha',
          version: '2.0.0',
          license: 'MIT',
          repository: null,
          texts: ['shared']
        },
        // 同じパッケージが2回渡されても、1つにする
        {
          ecosystem: 'npm',
          name: 'Alpha',
          version: '2.0.0',
          license: 'MIT',
          repository: null,
          texts: ['shared']
        }
      ]
    });

    expect(list.target).toBe('aarch64-apple-darwin');
    expect(list.rustMissing).toBe(false);
    expect(list.packages.map((pkg) => pkg.name)).toEqual(['Alpha', 'zeta']);
    expect(list.texts).toEqual(['shared', 'zeta only']);
    expect(list.packages[0].texts).toEqual([0]);
    expect(list.packages[1].texts).toEqual([0, 1]);
  });
});
