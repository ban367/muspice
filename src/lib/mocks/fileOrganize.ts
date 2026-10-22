/**
 * ファイルの整理（Rustの`src-tauri/src/file_organize.rs`）の、モック用の実装
 *
 * 書式の解釈と、整理後の場所の決め方を、Rust側と同じ規則で行う。実ファイルはないため、
 * 「移動先にファイルがある」は、ライブラリのほかの曲の場所と重なるかで判定し、歌詞ファイル・
 * フォルダの画像・空になったフォルダの扱いは再現しない。
 */
import type { AppError, OrganizeEntry, Track } from '#lib/types/models.js';

const MAX_TEMPLATE_LENGTH = 255;
const MAX_COMPONENT_CHARS = 80;

const FIELD_NAMES = [
  'title',
  'artist',
  'album',
  'albumartist',
  'genre',
  'year',
  'track',
  'disc',
  'filename'
] as const;
type Field = (typeof FIELD_NAMES)[number];

type Part = { text: string } | { field: Field } | { optional: Part[] };

export interface OrganizeTemplate {
  /** フォルダの階層（上から） */
  folders: Part[][];
  fileName: Part[];
  /** 今のフォルダのまま、ファイル名だけを変えるか */
  inPlace: boolean;
}

/** 値がない時に書く文字列（番号・年・ファイル名は、何も書かない） */
const FALLBACKS: Record<Field, string> = {
  title: 'Unknown Title',
  artist: 'Unknown Artist',
  albumartist: 'Unknown Artist',
  album: 'Unknown Album',
  genre: 'Unknown Genre',
  year: '',
  track: '',
  disc: '',
  filename: ''
};

function invalid(message: string): never {
  const error: AppError = { code: 'VALIDATION', message };
  throw error;
}

const isField = (name: string): name is Field => (FIELD_NAMES as readonly string[]).includes(name);

const hasField = (part: Part): boolean =>
  'field' in part || ('optional' in part && part.optional.some(hasField));

/** 書式の1階層（フォルダ名か、ファイル名）を解釈する */
function parseSegment(segment: string): Part[] {
  const parts: Part[] = [];
  let optional: Part[] | null = null;
  const chars = [...segment];

  for (let index = 0; index < chars.length; index++) {
    const char = chars[index];
    let part: Part;
    if (char === '%') {
      const end = chars.indexOf('%', index + 1);
      if (end < 0) invalid('「%」が閉じられていません');
      const name = chars.slice(index + 1, end).join('');
      index = end;
      if (name === '') {
        part = { text: '%' };
      } else {
        const field = name.toLowerCase();
        if (!isField(field)) invalid(`使えない項目です: %${name}%`);
        part = { field };
      }
    } else if (char === '[') {
      if (optional !== null) invalid('「[」の中に「[」は書けません');
      optional = [];
      continue;
    } else if (char === ']') {
      if (optional === null) invalid('「]」に対応する「[」がありません');
      part = { optional };
      optional = null;
    } else {
      part = { text: char };
    }

    const target: Part[] = optional ?? parts;
    const last = target.at(-1);
    // 続いた文字は、1つにまとめる
    if (last && 'text' in last && 'text' in part) last.text += part.text;
    else target.push(part);
  }

  if (optional !== null) invalid('「[」が閉じられていません');
  return parts;
}

/** 書式を解釈する（誤りは、`VALIDATION`のエラーを投げる） */
export function parseOrganizeTemplate(text: string): OrganizeTemplate {
  const trimmed = text.trim();
  if (trimmed === '') invalid('書式を入力してください');
  if ([...trimmed].length > MAX_TEMPLATE_LENGTH) {
    invalid(`書式が長すぎます（${MAX_TEMPLATE_LENGTH}文字以内）`);
  }

  const segments = trimmed.split(/[/\\]/);
  const inPlace = segments.length === 1;
  const fileNameText = segments.pop() ?? '';
  if (fileNameText.trim() === '') {
    invalid('最後の「/」の後ろに、ファイル名の書式を入れてください');
  }
  const fileName = parseSegment(fileNameText);
  if (!fileName.some(hasField)) {
    invalid('ファイル名（最後の「/」より後ろ）に、項目（%title%など）を入れてください');
  }
  const folders = segments.filter((segment) => segment.trim() !== '').map(parseSegment);
  return { folders, fileName, inPlace };
}

/** 曲の値（値がなければnull） */
function fieldValue(field: Field, track: Track, stem: string): string | null {
  const text = (value: string | null) => value?.trim() || null;
  const positive = (value: number | null) => (value !== null && value > 0 ? value : null);
  switch (field) {
    case 'title':
      return text(track.title) ?? text(stem);
    case 'artist':
      return text(track.artist);
    case 'album':
      return text(track.album);
    case 'albumartist':
      return text(track.albumArtist) ?? text(track.artist);
    case 'genre':
      return text(track.genre);
    case 'year':
      return positive(track.year)?.toString() ?? null;
    case 'track':
      return positive(track.trackNumber)?.toString().padStart(2, '0') ?? null;
    case 'disc':
      return positive(track.discNumber)?.toString() ?? null;
    case 'filename':
      return text(stem);
  }
}

function render(parts: Part[], track: Track, stem: string): string {
  return parts
    .map((part): string => {
      if ('text' in part) return part.text;
      if ('field' in part) return fieldValue(part.field, track, stem) ?? FALLBACKS[part.field];
      const hasValue = part.optional.some(
        (inner) => 'field' in inner && fieldValue(inner.field, track, stem) !== null
      );
      return hasValue ? render(part.optional, track, stem) : '';
    })
    .join('');
}

/** フォルダ名・ファイル名として使える形にする（Rustの`sanitize_component`の、主な規則） */
function sanitizeComponent(name: string): string {
  const replaced = name
    .normalize('NFC')
    // 制御文字と、ファイル名に使えない文字
    // eslint-disable-next-line no-control-regex
    .replace(/[\u0000-\u001f\u007f<>:"/\\|?*]/g, '_');
  const trim = (value: string) => value.replace(/^[.\s]+|[.\s]+$/g, '');
  const result = trim([...trim(replaced)].slice(0, MAX_COMPONENT_CHARS).join(''));
  return result === '' ? '_' : result;
}

/** パスを、フォルダ・ファイル名（拡張子なし）・拡張子に分ける */
function splitPath(filePath: string): { directory: string; stem: string; extension: string } {
  const slash = filePath.lastIndexOf('/');
  const fileName = filePath.slice(slash + 1);
  const dot = fileName.lastIndexOf('.');
  return {
    directory: filePath.slice(0, Math.max(slash, 0)),
    stem: dot > 0 ? fileName.slice(0, dot) : fileName,
    extension: dot > 0 ? fileName.slice(dot) : ''
  };
}

/**
 * 曲の、書式に従った場所（フォルダ名 … ファイル名を`/`でつないだもの）
 * @param extension - ファイル名に付ける拡張子（`.`から。省くと、元のファイルの拡張子）
 */
export function organizedRelativePath(
  template: OrganizeTemplate,
  track: Track,
  extension?: string
): string {
  const source = splitPath(track.filePath);
  const folders = template.folders
    .map((parts) => render(parts, track, source.stem))
    .filter((folder) => folder.trim() !== '')
    .map(sanitizeComponent);
  const rendered = render(template.fileName, track, source.stem);
  const fileName =
    sanitizeComponent(rendered.trim() === '' ? source.stem : rendered) +
    (extension ?? source.extension);
  return [...folders, fileName].join('/');
}

/**
 * 曲の、整理後の場所
 * @param baseFolder - 基準のフォルダ（曲のあるライブラリフォルダか、音楽フォルダ）
 * @param inPlace - `/`のない書式で、今のフォルダのままにするか（音楽フォルダへ集める時はしない）
 */
function destination(
  template: OrganizeTemplate,
  track: Track,
  baseFolder: string,
  inPlace: boolean
): string {
  const { directory } = splitPath(track.filePath);
  const base = template.inPlace && inPlace ? directory : baseFolder;
  return `${base}/${organizedRelativePath(template, track)}`;
}

/**
 * パスのあるボリューム（モックの決まり: `/Volumes/名前`の下はそのドライブ、ほかは起動ディスク）
 */
export function mockVolumeOf(path: string): string {
  const match = /^\/Volumes\/[^/]+/.exec(path);
  return match ? match[0] : '/';
}

/** 大文字と小文字・文字の形を区別しないで、同じ場所かを比べるためのキー */
const locationKey = (path: string) => path.normalize('NFC').toLowerCase();

/** 1曲の移動 */
export interface MockOrganizeMove {
  trackId: string;
  to: string;
  /** ボリュームをまたぐため、コピーしてから元を消すか（音楽フォルダへ集める時） */
  copy: boolean;
}

/**
 * 選んだ曲の、ファイルの整理の計画を立てる
 * @param tracks - 選んだ曲（この順に、一覧にする）
 * @param libraryFolders - ライブラリフォルダのパス
 * @param libraryTracks - ライブラリのすべての曲（その場所は、移動先にしない。見つからない曲の場所は
 *   `targetRegistered`、ほかは`targetExists`）
 * @param musicFolder - 音楽フォルダへ集める時は、音楽フォルダ（どの曲も、音楽フォルダからの書式どおりの
 *   場所へ移す）。nullなら、曲のあるライブラリフォルダの中で移す
 */
export function planOrganize(
  tracks: readonly Track[],
  libraryFolders: readonly string[],
  template: OrganizeTemplate,
  libraryTracks: readonly Pick<Track, 'filePath' | 'isMissing'>[],
  musicFolder: string | null = null
): { entries: OrganizeEntry[]; moves: MockOrganizeMove[] } {
  const occupied = new Set(
    libraryTracks.filter((t) => !t.isMissing).map((t) => locationKey(t.filePath))
  );
  const registered = new Set(
    libraryTracks.filter((t) => t.isMissing).map((t) => locationKey(t.filePath))
  );
  const entries: OrganizeEntry[] = [];
  const candidates: {
    index: number;
    trackId: string;
    from: string;
    to: string;
    copy: boolean;
  }[] = [];

  for (const track of tracks) {
    const folder = libraryFolders.find((path) => track.filePath.startsWith(`${path}/`));
    const base = musicFolder ?? folder;
    const relative = (path: string, from: string | undefined) =>
      from && path.startsWith(`${from}/`) ? path.slice(from.length + 1) : path;
    const entry: OrganizeEntry = {
      trackId: track.id,
      currentPath: relative(track.filePath, folder),
      newPath: null,
      status: 'move'
    };
    if (base === undefined) {
      entry.status = 'outsideLibrary';
    } else if (track.isMissing) {
      entry.status = 'missing';
    } else {
      const to = destination(template, track, base, musicFolder === null);
      entry.newPath = relative(to, base);
      if (to.normalize('NFC') === track.filePath.normalize('NFC')) {
        entry.status = 'unchanged';
      } else {
        const copy = musicFolder !== null && mockVolumeOf(track.filePath) !== mockVolumeOf(to);
        candidates.push({
          index: entries.length,
          trackId: track.id,
          from: track.filePath,
          to,
          copy
        });
      }
    }
    entries.push(entry);
  }

  const users: Record<string, number> = Object.create(null);
  for (const { to } of candidates) users[locationKey(to)] = (users[locationKey(to)] ?? 0) + 1;

  const moves: MockOrganizeMove[] = [];
  for (const { index, trackId, from, to, copy } of candidates) {
    const key = locationKey(to);
    if (users[key] > 1) {
      entries[index].status = 'sameTarget';
    } else if (occupied.has(key) && key !== locationKey(from)) {
      entries[index].status = 'targetExists';
    } else if (registered.has(key) && key !== locationKey(from)) {
      entries[index].status = 'targetRegistered';
    } else {
      moves.push({ trackId, to, copy });
    }
  }
  return { entries, moves };
}
