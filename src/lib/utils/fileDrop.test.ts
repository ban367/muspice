import { describe, expect, it } from 'vitest';
import { droppedNames, isFileDrag } from './fileDrop.js';
import { TRACK_DRAG_TYPE } from './trackDrag.js';

/** ドラッグのデータ（種類・項目・ファイル）の代わり */
function dataTransfer(options: {
  types?: string[];
  items?: { kind: string; entryName?: string; fileName?: string }[];
  files?: string[];
}): DataTransfer {
  return {
    types: options.types ?? [],
    items: (options.items ?? []).map((item) => ({
      kind: item.kind,
      webkitGetAsEntry: () => (item.entryName === undefined ? null : { name: item.entryName }),
      getAsFile: () => (item.fileName === undefined ? null : { name: item.fileName })
    })),
    files: (options.files ?? []).map((name) => ({ name }))
  } as unknown as DataTransfer;
}

describe('isFileDrag', () => {
  it('OSからのファイルのドラッグを見分ける', () => {
    expect(isFileDrag(dataTransfer({ types: ['Files'] }))).toBe(true);
    expect(isFileDrag(dataTransfer({ types: ['text/uri-list', 'Files'] }))).toBe(true);
  });

  it('アプリの中のドラッグ・文字のドラッグは、ファイルのドラッグではない', () => {
    expect(isFileDrag(dataTransfer({ types: [TRACK_DRAG_TYPE] }))).toBe(false);
    expect(isFileDrag(dataTransfer({ types: ['Files', TRACK_DRAG_TYPE] }))).toBe(false);
    expect(isFileDrag(dataTransfer({ types: ['text/plain'] }))).toBe(false);
    expect(isFileDrag(null)).toBe(false);
  });
});

describe('droppedNames', () => {
  it('ファイルとフォルダの名前を、項目から読む', () => {
    const names = droppedNames(
      dataTransfer({
        items: [
          { kind: 'file', entryName: 'a.flac', fileName: 'a.flac' },
          // フォルダは、エントリからしか名前を読めないことがある
          { kind: 'file', entryName: 'Album' },
          // エントリを読めない場合は、ファイルの名前
          { kind: 'file', fileName: 'b.mp3' },
          { kind: 'string' }
        ],
        files: ['a.flac', 'b.mp3']
      })
    );
    expect(names).toEqual(['a.flac', 'Album', 'b.mp3']);
  });

  it('項目から読めない場合は、ファイルの一覧から読む（重複は除く）', () => {
    expect(droppedNames(dataTransfer({ files: ['a.flac', 'a.flac', 'b.mp3'] }))).toEqual([
      'a.flac',
      'b.mp3'
    ]);
    expect(droppedNames(dataTransfer({}))).toEqual([]);
  });
});
