/**
 * ファイルの整理の画面の状態
 */
import type { Track } from '#lib/types/models.js';

class FileOrganizeDialogState {
  /** 整理する曲（一覧の順。閉じている間はnull） */
  tracks = $state.raw<Track[] | null>(null);

  /** 曲のファイルの整理の画面を開く（曲がなければ開かない） */
  open(tracks: Track[]): void {
    if (tracks.length > 0) this.tracks = tracks;
  }

  close(): void {
    this.tracks = null;
  }
}

/** ファイルの整理の画面（曲のメニューから開き、`(app)/+layout.svelte`が表示する） */
export const fileOrganizeDialog = new FileOrganizeDialogState();
