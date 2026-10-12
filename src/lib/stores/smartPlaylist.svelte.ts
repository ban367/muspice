/**
 * 自動プレイリストの編集画面の状態
 */
import type { Playlist } from '#lib/types/models.js';

class SmartPlaylistDialogState {
  /**
   * 編集画面の対象（閉じている間はnull）
   *
   * `playlist`がnullなら、新しい自動プレイリストを作る。
   */
  target = $state.raw<{ playlist: Playlist | null } | null>(null);

  /** 新しい自動プレイリストを作る画面を開く */
  openNew(): void {
    this.target = { playlist: null };
  }

  /** 自動プレイリストの条件を編集する画面を開く（通常のプレイリストでは開かない） */
  openEdit(playlist: Playlist): void {
    if (playlist.rules !== null) this.target = { playlist };
  }

  close(): void {
    this.target = null;
  }
}

/** 自動プレイリストの編集画面（サイドバー・プレイリストの画面から開き、`(app)/+layout.svelte`が表示する） */
export const smartPlaylistDialog = new SmartPlaylistDialogState();
