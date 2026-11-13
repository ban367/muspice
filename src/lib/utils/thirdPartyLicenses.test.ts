import { describe, expect, it, vi } from 'vitest';
import { licenseTexts, loadLicenseList, type LicenseList } from './thirdPartyLicenses.js';

const list: LicenseList = {
  target: 'aarch64-apple-darwin',
  rustMissing: false,
  packages: [
    {
      ecosystem: 'cargo',
      name: 'lofty',
      version: '0.25.4',
      license: 'MIT OR Apache-2.0',
      repository: 'https://github.com/Serial-ATA/lofty-rs',
      texts: [1, 0]
    }
  ],
  texts: ['Apache text', 'MIT text']
};

function respond(body: string, ok = true): typeof fetch {
  return vi.fn(async () => ({ ok, json: async () => JSON.parse(body) }) as Response);
}

describe('loadLicenseList', () => {
  it('成果物の直下の一覧を読む', async () => {
    const fetcher = respond(JSON.stringify(list));

    expect(await loadLicenseList(fetcher)).toEqual(list);
    expect(fetcher).toHaveBeenCalledWith('/third-party-licenses.json');
  });

  it('一覧がない環境（開発サーバー）では、nullを返す', async () => {
    expect(await loadLicenseList(respond('{}', false))).toBeNull();
    // ファイルがなく、代わりに画面のHTMLが返った
    expect(await loadLicenseList(respond('<!doctype html>'))).toBeNull();
    // 一覧の形をしていない
    expect(await loadLicenseList(respond('{"packages": 1}'))).toBeNull();
    expect(
      await loadLicenseList(
        vi.fn(async () => {
          throw new Error('offline');
        })
      )
    ).toBeNull();
  });
});

describe('licenseTexts', () => {
  it('パッケージのライセンスの文を、番号の順に返す', () => {
    expect(licenseTexts(list, list.packages[0])).toEqual(['MIT text', 'Apache text']);
  });

  it('ない番号は、読み飛ばす', () => {
    expect(licenseTexts(list, { ...list.packages[0], texts: [0, 9] })).toEqual(['Apache text']);
  });
});
