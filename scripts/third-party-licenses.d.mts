/** `third-party-licenses.mjs`の型（実装の説明は、そちらにある） */

/** 一覧に載せる1つのパッケージ（ライセンスの文は、文そのもの） */
export interface DescribedPackage {
  ecosystem: 'cargo' | 'npm';
  name: string;
  version: string;
  license: string;
  repository: string | null;
  texts: string[];
}

/** パッケージの、ライセンスを調べる元の情報 */
export interface PackageSource {
  ecosystem: 'cargo' | 'npm';
  name: string;
  version: string;
  license: string | null | undefined;
  repository: string | null | undefined;
  authors: string[];
  directory: string;
}

/** アプリに埋め込む一覧（ライセンスの文は`texts`にまとめ、パッケージからは番号で指す） */
export interface LicenseList {
  /** Rustの依存を集めた、OS・CPUの名前（集めていなければnull） */
  target: string | null;
  /** Rustの依存を集められなかったか（手元の`npm run build`で、`cargo`を呼べなかった） */
  rustMissing: boolean;
  packages: (Omit<DescribedPackage, 'texts'> & { texts: number[] })[];
  texts: string[];
}

export type LicenseExpression =
  { id: string } | { any: LicenseExpression[] } | { all: LicenseExpression[] };

export const ACCEPTED_LICENSES: Set<string>;
export function parseLicenseExpression(expression: string): LicenseExpression;
export function chooseLicenses(tree: LicenseExpression): string[] | null;
export function describePackage(source: PackageSource): DescribedPackage;
export function collectRustPackages(): { target: string; packages: DescribedPackage[] };
export function npmPackageDirectory(moduleId: string): string | null;
export function collectNpmPackages(
  moduleIds: Iterable<string>,
  extraPackages?: string[]
): DescribedPackage[];
export function buildLicenseList(input: {
  target: string | null;
  packages: DescribedPackage[];
  rustMissing?: boolean;
}): LicenseList;
