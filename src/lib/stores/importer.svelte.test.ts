import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ImportResult } from '#lib/types/models.js';

type ProgressListener = (event: {
  payload: { current: number; total: number; currentFile: string };
}) => void;

/** 進み具合の通知を聞いている関数 */
const listeners = new Set<ProgressListener>();

vi.mock('#lib/bindings.js', () => ({
  commands: {
    importFolder: vi.fn(),
    importFiles: vi.fn(),
    importDroppedFiles: vi.fn()
  },
  events: {
    importProgress: {
      listen: vi.fn(async (listener: ProgressListener) => {
        listeners.add(listener);
        return () => listeners.delete(listener);
      })
    }
  }
}));

import { commands } from '#lib/bindings.js';
import { importer } from './importer.svelte.js';
import { ui } from './ui.svelte.js';

const RESULT: ImportResult = {
  importedCount: 2,
  skippedCount: 1,
  relinkedCount: 0,
  errorCount: 0,
  errors: []
};

/** 非同期の処理（通知を聞き始める・コマンドの結果待ち）が進むのを待つ */
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  importer.reset();
  ui.isImportDialogOpen = false;
  vi.spyOn(console, 'error').mockImplementation(() => {});
});

describe('importer', () => {
  it('フォルダのインポートを実行し、進み具合と結果を持つ', async () => {
    let finish!: (result: ImportResult) => void;
    vi.mocked(commands.importFolder).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      })
    );
    importer.duplicateAction = 'Replace';

    const running = importer.importFolder('/Music/New');
    await flush();
    // ダイアログを開き、インポート中にする
    expect(ui.isImportDialogOpen).toBe(true);
    expect(importer.isImporting).toBe(true);
    expect(commands.importFolder).toHaveBeenCalledWith('/Music/New', 'Replace');

    for (const listener of listeners) {
      listener({ payload: { current: 1, total: 4, currentFile: 'a.flac' } });
    }
    expect(importer.progress).toBe(25);
    expect(importer.processedFiles).toBe(1);
    expect(importer.totalFiles).toBe(4);
    expect(importer.currentFile).toBe('a.flac');

    finish(RESULT);
    await running;
    expect(importer.isImporting).toBe(false);
    expect(importer.progress).toBe(100);
    expect(importer.result).toEqual(RESULT);
    // 通知を聞くのをやめる
    expect(listeners.size).toBe(0);
  });

  it('ファイルを選ばずに閉じたら、結果もエラーもなく、ダイアログも開かない', async () => {
    vi.mocked(commands.importFiles).mockResolvedValue(null);

    await importer.importFiles();

    expect(commands.importFiles).toHaveBeenCalledWith('Skip');
    expect(importer.isImporting).toBe(false);
    expect(importer.result).toBeNull();
    expect(importer.errorMessage).toBe('');
    expect(ui.isImportDialogOpen).toBe(false);
  });

  it('ファイルを選ぶインポートは、インポートが始まってからダイアログを開く', async () => {
    let finish!: (result: ImportResult) => void;
    vi.mocked(commands.importFiles).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      })
    );

    const running = importer.importFiles();
    await flush();
    // ファイルを選んでいる間は、まだ開かない
    expect(ui.isImportDialogOpen).toBe(false);

    for (const listener of listeners) {
      listener({ payload: { current: 1, total: 2, currentFile: 'a.flac' } });
    }
    expect(ui.isImportDialogOpen).toBe(true);

    finish(RESULT);
    await running;
    expect(importer.result).toEqual(RESULT);
  });

  it('ドロップされたものの名前を渡し、失敗はエラーとして持つ', async () => {
    vi.mocked(commands.importDroppedFiles).mockRejectedValue({
      code: 'VALIDATION',
      message: 'インポートできる音楽ファイルがありません'
    });

    await importer.importDropped(['cover.jpg']);

    expect(commands.importDroppedFiles).toHaveBeenCalledWith(['cover.jpg']);
    expect(ui.isImportDialogOpen).toBe(true);
    expect(importer.result).toBeNull();
    expect(importer.errorMessage).toContain('インポートできる音楽ファイルがありません');
    expect(importer.isImporting).toBe(false);
  });

  it('インポートの途中では、別のインポートを始めない', async () => {
    let finish!: (result: ImportResult) => void;
    vi.mocked(commands.importFolder).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      })
    );
    const running = importer.importFolder('/Music/New');
    await flush();

    await importer.importDropped(['a.flac']);
    expect(commands.importDroppedFiles).not.toHaveBeenCalled();
    // 途中では、表示を消さない
    importer.reset();
    expect(importer.isImporting).toBe(true);

    finish(RESULT);
    await running;
    importer.reset();
    expect(importer.result).toBeNull();
    expect(importer.progress).toBe(0);
  });
});
