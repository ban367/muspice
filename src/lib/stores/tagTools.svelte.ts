/**
 * タグの一括ツールの画面の状態
 */
import type { Track } from '#lib/types/models.js';

class TagToolsDialogState {
  /** 一括ツールで扱う曲（一覧の順。閉じている間はnull） */
  tracks = $state.raw<Track[] | null>(null);

  /** 曲のタグの一括ツールを開く（曲がなければ開かない） */
  open(tracks: Track[]): void {
    if (tracks.length > 0) this.tracks = tracks;
  }

  close(): void {
    this.tracks = null;
  }
}

/** タグの一括ツールの画面（メニューから開き、`(app)/+layout.svelte`が表示する） */
export const tagToolsDialog = new TagToolsDialogState();
