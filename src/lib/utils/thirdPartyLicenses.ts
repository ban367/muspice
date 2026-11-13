/**
 * アプリに組み込んだ依存ライブラリの、ライセンスの一覧（ADR-091）
 *
 * 一覧は、ビルドの時に作って、成果物の直下の`third-party-licenses.json`に置く
 * （`vite.config.js`のプラグインと`scripts/third-party-licenses.mjs`）。アプリは、「Muspice について」から
 * 開いた時に読む。開発サーバー（`npm run dev`・`dev:mock`）にはファイルがないため、読めない。
 */

/** 一覧の1つのパッケージ（ライセンスの文は、`LicenseList.texts`の番号で指す） */
export interface LicensedPackage {
  ecosystem: 'cargo' | 'npm';
  name: string;
  version: string;
  /** ライセンスの表記（SPDXの式） */
  license: string;
  /** 入手先（分からなければnull） */
  repository: string | null;
  texts: number[];
}

/** ライセンスの一覧 */
export interface LicenseList {
  /** Rustの依存を集めた、OS・CPUの名前（集めていなければnull） */
  target: string | null;
  /** Rustの依存が入っていないか（`cargo`を呼べない環境で、フロントだけをビルドした） */
  rustMissing: boolean;
  packages: LicensedPackage[];
  /** ライセンスの文（同じ文は1つにまとめてある） */
  texts: string[];
}

/** 一覧を置いてある場所（アプリは、いつもルートで配る） */
const LICENSES_PATH = '/third-party-licenses.json';

/** 一覧の形をしているか（ファイルがない時に返る、ほかの内容を弾く） */
function isLicenseList(value: unknown): value is LicenseList {
  if (typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return Array.isArray(record.packages) && Array.isArray(record.texts);
}

/**
 * ライセンスの一覧を読む
 * @returns 一覧（開発サーバーなど、一覧がない環境ではnull）
 */
export async function loadLicenseList(fetcher: typeof fetch = fetch): Promise<LicenseList | null> {
  try {
    const response = await fetcher(LICENSES_PATH);
    if (!response.ok) return null;
    const list: unknown = await response.json();
    return isLicenseList(list) ? list : null;
  } catch {
    // ファイルがなく、代わりにHTMLが返った（JSONとして読めない）場合など
    return null;
  }
}

/** パッケージの、ライセンスの文（番号を、文に直す） */
export function licenseTexts(list: LicenseList, pkg: LicensedPackage): string[] {
  return pkg.texts.map((index) => list.texts[index]).filter((text) => text !== undefined);
}
