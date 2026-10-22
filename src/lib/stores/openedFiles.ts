/**
 * 「このアプリで開く」などで開かれた曲の再生
 *
 * Finderの「このアプリで開く」・Dockのアイコンへのドロップで開かれたファイルは、Rust側が
 * ライブラリへインポートしてから知らせる（`TracksOpened`。`src-tauri/src/opened_files.rs`）。
 * ここでは、その曲を受け取り、再生キューにして再生する。起動の途中に開かれた分（知らせを
 * 聞き始める前に届いた分）も、起動した時に受け取る。
 */
import { commands, events } from '#lib/bindings.js';
import { handleError, showWarning } from './error.svelte.js';
import { playTrackFromQueue } from './player.svelte.js';
import { m } from '#lib/i18n/i18n.svelte.js';
import { stopListening } from '#lib/utils/listen.js';

/** 開かれた曲を受け取り、再生する */
async function playOpenedFiles(): Promise<void> {
  const opened = await commands.takeOpenedFiles();
  if (opened.errors.length > 0) {
    showWarning(m.openedFiles.failed(opened.errors[0], opened.errors.length - 1));
  }
  if (opened.tracks.length > 0) playTrackFromQueue(opened.tracks, 0);
}

/**
 * 開かれた曲の知らせを聞き始める
 * @returns 聞くのをやめる関数
 */
export function watchOpenedFiles(): () => void {
  const receive = () => {
    playOpenedFiles().catch((error) => handleError(error, m.openedFiles.context));
  };
  const unlisten = events.tracksOpened.listen(receive);
  // 聞き始めた後に、それまでに開かれた分（起動の途中・聞き始める手続きの間に届いた分）を受け取る
  unlisten.then(receive, (error) => handleError(error, m.openedFiles.context));
  return () => stopListening(unlisten);
}
