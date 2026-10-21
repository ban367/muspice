import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
  ImportOutcome,
  ImportProgress,
  ImportResult,
  PendingFolderImport
} from '#lib/types/models.js';

type ProgressListener = (event: { payload: ImportProgress }) => void;

/** 進み具合の通知を聞いている関数 */
const listeners = new Set<ProgressListener>();

vi.mock('#lib/bindings.js', () => ({
  commands: {
    importFolder: vi.fn(),
    importFiles: vi.fn(),
    importDroppedFiles: vi.fn(),
    importPendingFolders: vi.fn(),
    cancelPendingImport: vi.fn(async () => {}),
    setMusicFolder: vi.fn()
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
import { notifications } from './error.svelte.js';
import { importer } from './importer.svelte.js';
import { ui } from './ui.svelte.js';

const RESULT: ImportResult = {
  importedCount: 2,
  skippedCount: 1,
  relinkedCount: 0,
  errorCount: 0,
  errors: [],
  copiedCount: 0,
  notCopied: []
};
const IMPORTED: ImportOutcome = { kind: 'imported', result: RESULT };

/** 参照するかコピーするかを選ぶ、預かったフォルダ */
const PENDING: PendingFolderImport = {
  folders: ['/Volumes/Backup/Album'],
  trackCount: 12,
  totalBytes: 480_000_000,
  copyCount: 12,
  copyBytes: 480_000_000,
  musicFolder: '/Users/a/Music',
  musicFolderExists: true,
  freeBytes: 100_000_000_000
};

/** 進み具合を知らせる（読む段階） */
const progressEvent = (current: number, total: number, currentFile: string) => ({
  payload: { phase: 'reading' as const, current, total, currentFile }
});

/** 非同期の処理（通知を聞き始める・コマンドの結果待ち）が進むのを待つ */
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  importer.reset();
  notifications.clear();
  ui.isImportDialogOpen = false;
  vi.spyOn(console, 'error').mockImplementation(() => {});
});

describe('importer', () => {
  it('フォルダを選ぶインポートを実行し、進み具合と結果を持つ', async () => {
    let finish!: (outcome: ImportOutcome) => void;
    vi.mocked(commands.importFolder).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      })
    );
    importer.duplicateAction = 'Replace';

    const running = importer.importFolder();
    await flush();
    // ダイアログを開き、インポート中にする（フォルダを選ぶダイアログは、Rust側が開く）
    expect(ui.isImportDialogOpen).toBe(true);
    expect(importer.isImporting).toBe(true);
    expect(commands.importFolder).toHaveBeenCalledWith('Replace');

    for (const listener of listeners) {
      listener(progressEvent(1, 4, 'a.flac'));
    }
    expect(importer.progress).toBe(25);
    expect(importer.processedFiles).toBe(1);
    expect(importer.totalFiles).toBe(4);
    expect(importer.currentFile).toBe('a.flac');

    finish(IMPORTED);
    await running;
    expect(importer.isImporting).toBe(false);
    expect(importer.progress).toBe(100);
    expect(importer.result).toEqual(RESULT);
    // 通知を聞くのをやめる
    expect(listeners.size).toBe(0);
  });

  it('フォルダを選ばずに閉じたら、結果もエラーもなく、インポート中でなくなる', async () => {
    vi.mocked(commands.importFolder).mockResolvedValue(null);

    await importer.importFolder();

    expect(commands.importFolder).toHaveBeenCalledWith('Skip');
    expect(importer.isImporting).toBe(false);
    expect(importer.progress).toBe(0);
    expect(importer.result).toBeNull();
    expect(importer.errorMessage).toBe('');
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
    let finish!: (outcome: ImportOutcome) => void;
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
      listener(progressEvent(1, 2, 'a.flac'));
    }
    expect(ui.isImportDialogOpen).toBe(true);

    finish(IMPORTED);
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

  it('空の音楽フォルダを選んだら、ダイアログを開かずに知らせる', async () => {
    vi.mocked(commands.setMusicFolder).mockResolvedValue({
      kind: 'set',
      path: '/Users/a/Music',
      imported: { ...RESULT, importedCount: 0, skippedCount: 0 }
    });

    await importer.setMusicFolder('system');

    expect(commands.setMusicFolder).toHaveBeenCalledWith('system');
    expect(ui.isImportDialogOpen).toBe(false);
    expect(importer.result).toBeNull();
    expect(notifications.items.map((n) => [n.type, n.message])).toEqual([
      ['info', '音楽フォルダを「/Users/a/Music」にしました']
    ]);
  });

  it('曲のある音楽フォルダを選んだら、読み込みの進み具合と結果をダイアログに出す', async () => {
    let finish!: (change: Awaited<ReturnType<typeof commands.setMusicFolder>>) => void;
    vi.mocked(commands.setMusicFolder).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      })
    );

    const running = importer.setMusicFolder('pick');
    await flush();
    // フォルダを選んでいる間は、まだ開かない
    expect(ui.isImportDialogOpen).toBe(false);
    for (const listener of listeners) {
      listener(progressEvent(1, 2, 'a.flac'));
    }
    expect(ui.isImportDialogOpen).toBe(true);

    finish({ kind: 'set', path: '/Volumes/Music', imported: RESULT });
    await running;
    expect(importer.result).toEqual(RESULT);
    expect(notifications.items[0].message).toBe(
      '音楽フォルダを「/Volumes/Music」にし、2曲を読み込みました'
    );
  });

  it('ライブラリフォルダの中のフォルダを選んだら、外側のフォルダを案内する', async () => {
    vi.mocked(commands.setMusicFolder).mockResolvedValue({
      kind: 'insideLibraryFolder',
      path: '/Users/a/Music/New',
      libraryFolderPath: '/Users/a/Music'
    });

    await importer.setMusicFolder('pick');

    expect(ui.isImportDialogOpen).toBe(false);
    expect(notifications.items.map((n) => n.type)).toEqual(['warning']);
    expect(notifications.items[0].message).toContain('/Users/a/Music');
  });

  it('インポートの途中では、別のインポートを始めない', async () => {
    let finish!: (outcome: ImportOutcome) => void;
    vi.mocked(commands.importFolder).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      })
    );
    const running = importer.importFolder();
    await flush();

    await importer.importDropped(['a.flac']);
    expect(commands.importDroppedFiles).not.toHaveBeenCalled();
    // 途中では、表示を消さない
    importer.reset();
    expect(importer.isImporting).toBe(true);

    finish(IMPORTED);
    await running;
    importer.reset();
    expect(importer.result).toBeNull();
    expect(importer.progress).toBe(0);
  });
  it('フォルダの取り込み方を選ぶ時は、預かったフォルダを持ち、選んだ方法で取り込む', async () => {
    vi.mocked(commands.importFolder).mockResolvedValue({
      kind: 'chooseFolderMode',
      pending: PENDING
    });
    importer.duplicateAction = 'Replace';

    await importer.importFolder();

    expect(ui.isImportDialogOpen).toBe(true);
    expect(importer.pending).toEqual(PENDING);
    expect(importer.result).toBeNull();
    expect(importer.isImporting).toBe(false);

    const copied = { ...RESULT, importedCount: 12, copiedCount: 12 };
    vi.mocked(commands.importPendingFolders).mockResolvedValue({
      kind: 'imported',
      result: copied
    });
    await importer.importPending('copy');

    expect(commands.importPendingFolders).toHaveBeenCalledWith('copy', 'Replace');
    expect(importer.result).toEqual(copied);
    expect(importer.pending).toBeNull();
    // 取り込んだ後は、Rust側も預かっていないため、やめる必要はない
    importer.reset();
    expect(commands.cancelPendingImport).not.toHaveBeenCalled();
  });

  it('音楽フォルダへコピーできなければ理由を出し、預かったフォルダは残す', async () => {
    vi.mocked(commands.importDroppedFiles).mockResolvedValue({
      kind: 'chooseFolderMode',
      pending: PENDING
    });
    await importer.importDropped(['Album']);
    vi.mocked(commands.importPendingFolders).mockResolvedValue({
      kind: 'notEnoughSpace',
      requiredBytes: 2 * 1024 ** 3,
      freeBytes: 1024 ** 3
    });

    await importer.importPending('copy');

    expect(importer.errorMessage).toContain('空き容量が足りません');
    expect(importer.errorMessage).toContain('2.00 GB');
    // 参照して取り込むこともできる
    expect(importer.pending).toEqual(PENDING);
    expect(importer.isImporting).toBe(false);
  });

  it('音楽フォルダが見つからなければ、ファイルの取り込みも理由を出す', async () => {
    vi.mocked(commands.importFiles).mockResolvedValue({
      kind: 'musicFolderMissing',
      path: '/Volumes/Music'
    });

    await importer.importFiles();

    expect(ui.isImportDialogOpen).toBe(true);
    expect(importer.result).toBeNull();
    expect(importer.errorMessage).toContain('音楽フォルダ（/Volumes/Music）が見つかりません');
  });

  it('ダイアログを閉じたら、預かったフォルダの取り込みをやめる', async () => {
    vi.mocked(commands.importFolder).mockResolvedValue({
      kind: 'chooseFolderMode',
      pending: PENDING
    });
    await importer.importFolder();

    importer.reset();

    expect(commands.cancelPendingImport).toHaveBeenCalledTimes(1);
    expect(importer.pending).toBeNull();
  });

  it('コピーの段階を、進み具合に持つ', async () => {
    let finish!: (outcome: ImportOutcome) => void;
    vi.mocked(commands.importFiles).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      })
    );
    const running = importer.importFiles();
    await flush();

    for (const listener of listeners) {
      listener({ payload: { phase: 'copying', current: 1, total: 2, currentFile: 'a.flac' } });
    }
    expect(importer.phase).toBe('copying');
    expect(importer.progress).toBe(50);

    finish(IMPORTED);
    await running;
  });
});
