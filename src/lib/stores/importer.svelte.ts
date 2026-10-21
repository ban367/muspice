/**
 * インポートの実行と、その状態（インポートダイアログが表示する）
 *
 * インポートの始め方は3つある。どれも同じダイアログに、進み具合と結果を表示する。
 *
 * - フォルダを選ぶ（`importFolder`。フォルダを選ぶダイアログは、Rust側が開く）
 * - ファイルを選ぶ（`importFiles`。ファイルを選ぶダイアログは、Rust側が開く）
 * - ウィンドウへドロップする（`importDropped`。パスはRust側が読む。ADR-050）
 *
 * ライブラリが空の画面で音楽フォルダを選んだ時（`setMusicFolder`）も、中の曲の読み込みを、
 * 同じダイアログに表示する。
 *
 * 曲の一覧の更新は、Rust側の通知（`LibraryChanged`）で行う（レイアウトが受け取る）。
 */
import { commands, events } from '#lib/bindings.js';
import type { DuplicateAction, ImportResult, MusicFolderSource } from '#lib/types/models.js';
import { musicFolderNotice } from '#lib/utils/musicFolder.js';
import { showSuccess, showWarning, toErrorMessage } from './error.svelte.js';
import { ui } from './ui.svelte.js';
import { m } from '#lib/i18n/i18n.svelte.js';

class Importer {
  /** 登録済みのファイルの扱い（フォルダ・ファイルを選ぶインポートで使う） */
  duplicateAction = $state<DuplicateAction>('Skip');
  isImporting = $state(false);
  /** 進み具合（0〜100） */
  progress = $state(0);
  /** 読んでいるファイルの名前 */
  currentFile = $state('');
  totalFiles = $state(0);
  processedFiles = $state(0);
  /** 終わったインポートの結果（まだ・失敗した場合はnull） */
  result = $state.raw<ImportResult | null>(null);
  errorMessage = $state('');

  /**
   * フォルダを選ぶダイアログを開き、選んだフォルダをインポートする（選ばずに閉じたら、何もしない）
   *
   * インポートダイアログから始める（ダイアログは開いたまま、進み具合と結果を表示する）。
   */
  importFolder(): Promise<void> {
    return this.run(() => commands.importFolder(this.duplicateAction));
  }

  /**
   * ファイルを選ぶダイアログを開き、選んだファイルをインポートする（選ばずに閉じたら、何もしない）
   *
   * インポートダイアログは、ファイルを選んでインポートが始まってから開く（メニューから始めて、
   * 選ばずに閉じた場合に、ダイアログだけが残らないようにする）。
   */
  importFiles(): Promise<void> {
    return this.run(() => commands.importFiles(this.duplicateAction), {
      showDialogWhenStarted: true
    });
  }

  /**
   * ウィンドウへドロップされたファイル・フォルダをインポートする
   * @param names - ドロップされたものの名前（パスは、Rust側がOSから読む）
   */
  importDropped(names: string[]): Promise<void> {
    return this.run(() => commands.importDroppedFiles(names));
  }

  /**
   * 音楽フォルダを選び、中の曲を読み込む（ライブラリが空の画面から始める）
   *
   * フォルダは、OSの音楽フォルダか、Rust側が開くダイアログで選ぶ。曲があれば、インポート
   * ダイアログに進み具合と結果を表示する。どの場合も、音楽フォルダにしたこと（できなかった
   * 場合は、その理由）をトーストで知らせる（空のフォルダでは、ダイアログを開かない）。
   */
  setMusicFolder(source: MusicFolderSource): Promise<void> {
    return this.run(
      async () => {
        const change = await commands.setMusicFolder(source);
        if (!change) return null;
        const notice = musicFolderNotice(change);
        if (notice.kind === 'success') showSuccess(notice.message);
        else showWarning(notice.message);
        // 曲を読み込んだ（読めなかったファイルがあった）場合だけ、結果をダイアログに出す
        const imported = change.kind === 'set' ? change.imported : null;
        return imported && imported.importedCount + imported.relinkedCount + imported.errorCount > 0
          ? imported
          : null;
      },
      { showDialogWhenStarted: true }
    );
  }

  /** 表示を、インポートを始める前の状態に戻す（インポート中は何もしない） */
  reset(): void {
    if (this.isImporting) return;
    this.duplicateAction = 'Skip';
    this.progress = 0;
    this.currentFile = '';
    this.totalFiles = 0;
    this.processedFiles = 0;
    this.result = null;
    this.errorMessage = '';
  }

  /**
   * インポートを実行し、進み具合と結果をダイアログに表示する
   * @param start - インポートを始める関数（nullを返したら、取り消されたものとして扱う）
   * @param options.showDialogWhenStarted - ダイアログを、最初の進み具合か結果が届いてから開くか
   *   （しない場合は、すぐに開く）
   */
  private async run(
    start: () => Promise<ImportResult | null>,
    options: { showDialogWhenStarted?: boolean } = {}
  ): Promise<void> {
    // インポートは、同時に1つだけ
    if (this.isImporting) return;
    const showDialog = () => {
      ui.isImportDialogOpen = true;
    };
    if (!options.showDialogWhenStarted) showDialog();
    this.isImporting = true;
    this.progress = 0;
    this.currentFile = '';
    this.totalFiles = 0;
    this.processedFiles = 0;
    this.errorMessage = '';
    this.result = null;

    let unlisten: (() => void) | null = null;
    try {
      unlisten = await events.importProgress.listen((event) => {
        const { current, total, currentFile } = event.payload;
        this.processedFiles = current;
        this.totalFiles = total;
        this.currentFile = currentFile;
        this.progress = total > 0 ? Math.round((current / total) * 100) : 0;
        showDialog();
      });

      const result = await start();
      if (result) {
        this.progress = 100;
        this.result = result;
        showDialog();
      } else {
        this.progress = 0;
      }
    } catch (error) {
      console.error('インポートエラー:', error);
      this.errorMessage = m.importDialog.importFailed(toErrorMessage(error));
      this.progress = 0;
      showDialog();
    } finally {
      this.isImporting = false;
      unlisten?.();
    }
  }
}

export const importer = new Importer();
