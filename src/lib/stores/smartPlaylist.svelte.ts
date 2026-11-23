/**
 * オートプレイリストの編集画面の状態
 */
import type { SmartPlaylist } from '#lib/types/models.js';
import { promptText } from '#lib/utils/dialog.svelte.js';
import { toSafeString, validatePlaylistName } from '#lib/utils/validation.js';
import { m } from '#lib/i18n/i18n.svelte.js';

class SmartPlaylistDialogState {
  /**
   * 編集画面の対象（閉じている間はnull）
   *
   * `smartPlaylist`がnullなら、新しいオートプレイリストを作る。
   */
  target = $state.raw<{ smartPlaylist: SmartPlaylist | null } | null>(null);

  /** 新しいオートプレイリストを作る画面を開く */
  openNew(): void {
    this.target = { smartPlaylist: null };
  }

  /** オートプレイリストの名前と条件を編集する画面を開く */
  openEdit(smartPlaylist: SmartPlaylist): void {
    this.target = { smartPlaylist };
  }

  close(): void {
    this.target = null;
  }
}

/** オートプレイリストの編集画面（サイドバー・オートプレイリストの画面から開き、`(app)/+layout.svelte`が表示する） */
export const smartPlaylistDialog = new SmartPlaylistDialogState();

/**
 * 「プレイリストとして保存」で作るプレイリストの名前を尋ねる（キャンセルした場合はnull）
 *
 * 初期値は、オートプレイリストの名前。
 */
export async function promptSaveAsPlaylistName(defaultName: string): Promise<string | null> {
  const name = await promptText({
    title: m.smartPlaylist.saveAsPlaylistTitle,
    description: m.smartPlaylist.saveAsPlaylistHint,
    label: m.smartPlaylist.saveAsPlaylistLabel,
    defaultValue: defaultName,
    confirmLabel: m.smartPlaylist.saveAsPlaylistConfirm,
    validate: (value) => validatePlaylistName(value).error ?? null
  });
  return name === null ? null : toSafeString(name, 100);
}
