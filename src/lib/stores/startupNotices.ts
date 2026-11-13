/**
 * 起動の途中で起きた問題の知らせ（ADR-088）
 *
 * ライブラリのDBを開けずに空のライブラリで起動した・用意してあった復元を当てられなかった、などは、
 * Rust側が預かっている（`src-tauri/src/startup.rs`）。メインウィンドウが起動した時に受け取り、
 * ダイアログで知らせる（トーストは数秒で消えるため使わない）。
 */
import { commands } from '#lib/bindings.js';
import type { StartupNotice } from '#lib/types/models.js';
import { m } from '#lib/i18n/i18n.svelte.js';
import { showNotice } from '#lib/utils/dialog.svelte.js';

/** 知らせの文 */
export function startupNoticeMessage(notice: StartupNotice): string {
  switch (notice.kind) {
    case 'databaseQuarantined':
      return m.startup.databaseQuarantined(notice.quarantinedPath, notice.hasBackups);
    case 'restoreFailed':
      return m.startup.restoreFailed(notice.detail);
  }
}

/** 預かってある起動時の知らせを受け取り、1つずつダイアログで知らせる（なければ何もしない） */
export async function showStartupNotices(): Promise<void> {
  const notices = await commands.takeStartupNotices();
  for (const notice of notices) {
    await showNotice(startupNoticeMessage(notice), m.startup.title);
  }
}
