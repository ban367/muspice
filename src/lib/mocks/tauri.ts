/**
 * ブラウザ上でTauri IPCをモックする（`npm run dev:mock`専用）
 *
 * `@tauri-apps/api/mocks`で`window.__TAURI_INTERNALS__`を差し替え、
 * アプリのコマンドはインメモリバックエンドへ、プラグイン（event・dialog・window・app）は
 * ブラウザ内で完結する簡易実装へ振り分ける。状態はメモリ上にのみ保持し、
 * リロードするとフィクスチャの初期状態に戻る（再生状態（音量・再生キューなど）だけは、
 * 起動時の復元を確認できるよう`sessionStorage`に保存し、タブを閉じるまで残る）。
 *
 * ネイティブメニューは存在しないため、メニュー由来のイベントは開発者ツールから
 * `window.__MUSPICE_MOCK__.emit('toggle-sidebar')`のように発火させる。「再生」メニューと
 * OSのメディアキーの操作は、`emit('playback-control', { type: 'next' })`のように発火させる。
 * OSのNow Playingの表示もないため、フロントエンドが伝えた内容は`nowPlaying()`で確認する。
 *
 * 数万曲のライブラリでの動作は、URLに`?mockTracks=50000`を付けて開くと確認できる
 * （フィクスチャに加えて、指定した数のトラックを生成する）。
 *
 * 音は鳴らない。再生エンジンのコマンドと通知は`playbackEngine.ts`が再現し、再生位置と
 * 曲の切り替わりだけが、時計に合わせて進む。
 */
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import type { NowPlayingUpdate } from '#lib/types/models.js';
import tauriConfig from '../../../src-tauri/tauri.conf.json';
import {
  createMockBackend,
  MOCK_EVENT_NAMES,
  type MockAlbumArtPickMode,
  type MockFileImportMode,
  type MockM3uImportMode
} from './backend';

/** アートがないトラックに返すURL（読み込みエラーになり、実アプリの404と同じ扱いになる） */
const MISSING_ALBUM_ART_URL = 'data:image/png;base64,';

/** `mockTracks`で生成できるトラックの数の上限 */
const MAX_EXTRA_TRACKS = 200_000;

/** 開発者ツールから操作するためのハンドル */
interface MuspiceMockHandle {
  /** バックエンド（Rust）からのイベント送信を再現する */
  emit(event: string, payload?: unknown): void;
  /** 確認ダイアログ（dialogプラグインのconfirm）の回答を切り替える。既定はOK */
  setConfirmResult(result: boolean): void;
  /**
   * フォルダを選ぶダイアログ（Rust側が開く。フォルダのインポート・転送先デバイスの登録・
   * 転送先の変更）で選んだことにするパスを切り替える（nullは、選ばずに閉じた）
   *
   * 既定のパスはライブラリフォルダの中のため、転送先デバイスの追加を確認するときは
   * ライブラリの外のパス（例: `/Volumes/NEW_SD`）にする。
   */
  setFolderResult(path: string | null): void;
  /** OSのNow Playingへ伝えたことになっている内容（何も再生していなければnull） */
  nowPlaying(): NowPlayingUpdate | null;
  /**
   * M3Uの読み込みで、選んだことにするファイルを切り替える
   * （`partial`: 対応が付かない行がある（既定）・`clean`: すべて対応が付く・`cancel`: 選ばなかった）
   */
  setM3uImportMode(mode: MockM3uImportMode): void;
  /**
   * アルバムアートの埋め込みで、選んだことにする画像を切り替える
   * （`pick`: 埋め込める画像（既定）・`cancel`: 選ばなかった・`tooLarge`: 大きすぎる画像）
   */
  setAlbumArtPickMode(mode: MockAlbumArtPickMode): void;
  /**
   * ファイルを選ぶインポートで、ファイルを選んだことにするかを切り替える
   * （`pick`: 2つの音楽ファイルを選んだ（既定）・`cancel`: 選ばなかった）
   */
  setFileImportMode(mode: MockFileImportMode): void;
  /** Finderの「このアプリで開く」で、音楽ファイルが開かれたことにする（インポートして再生される） */
  openFiles(fileNames: string[]): void;
  /** 重複した曲を、ライブラリに足す（重複した曲の画面と、ファイルの整理の確認用） */
  addDuplicateTracks(): void;
  /** 形式の変換で、書き出すフォルダを選ぶダイアログの結果を切り替える（nullは、選ばなかった） */
  setConvertExportFolder(path: string | null): void;
  /**
   * 読み込まないフォルダの追加で、ダイアログで選んだことにするフォルダを切り替える
   * （絶対パス。nullは、選ばなかった。既定は、ライブラリフォルダの中の曲のある最初のフォルダ）
   */
  setExclusionFolder(path: string | null): void;
  /**
   * 音楽フォルダに新しく登録するフォルダで、見つかったことにする曲の数を切り替える
   * （0は、空のフォルダ。既定はフォルダのインポートと同じ数）
   */
  setMusicFolderFileCount(count: number): void;
  /** 音楽フォルダのドライブの空き容量（バイト）を切り替える（取り込む曲のコピーの確認用） */
  setMusicFolderFreeBytes(bytes: number): void;
}

/** モックで使う`__TAURI_INTERNALS__`の一部（公開型がないため最小限を定義する） */
interface TauriInternals {
  runCallback(id: number, data: unknown): void;
  convertFileSrc(filePath: string, protocol?: string): string;
}

declare global {
  interface Window {
    __MUSPICE_MOCK__?: MuspiceMockHandle;
  }
}

function tauriInternals(): TauriInternals {
  return (window as unknown as { __TAURI_INTERNALS__: TauriInternals }).__TAURI_INTERNALS__;
}

/**
 * dialogプラグインの`message`コマンドへの回答（押されたボタンのラベル）を返す
 *
 * `confirm`はOK側のラベルが返ったかどうかで結果を判定する。
 */
function answerMessageDialog(buttons: unknown, confirmed: boolean): string {
  if (typeof buttons === 'object' && buttons !== null && 'OkCancelCustom' in buttons) {
    const [ok, cancel] = buttons.OkCancelCustom as [string, string];
    return confirmed ? ok : cancel;
  }
  if (buttons === 'OkCancel') return confirmed ? 'Ok' : 'Cancel';
  if (buttons === 'YesNo') return confirmed ? 'Yes' : 'No';
  return 'Ok';
}

/**
 * URLの`mockTracks`で指定された、追加で生成するトラックの数を返す
 *
 * 数万曲のライブラリでの動作を確認する時に、`/library/songs?mockTracks=50000`のように開く。
 */
function requestedExtraTrackCount(): number {
  const requested = Number(new URLSearchParams(window.location.search).get('mockTracks'));
  return Number.isFinite(requested)
    ? Math.min(Math.max(0, Math.floor(requested)), MAX_EXTRA_TRACKS)
    : 0;
}

export function setupTauriMock(): void {
  // イベントの購読はここで管理する。公式のshouldMockEventsはunlisten時に
  // 購読を解除しないため、解除済みのコールバックへ送信して警告が出てしまう
  const listeners = new Map<string, Set<number>>();
  let confirmResult = true;

  function emit(event: string, payload?: unknown): void {
    for (const handlerId of [...(listeners.get(event) ?? [])]) {
      tauriInternals().runCallback(handlerId, { event, id: handlerId, payload });
    }
  }

  const backend = createMockBackend({
    // バックエンドは`events`のキーで送る。IPCでは、`bindings.ts`と同じ名前（ケバブケース）で届ける
    emit: (event, payload) => emit(MOCK_EVENT_NAMES[event], payload),
    extraTrackCount: requestedExtraTrackCount(),
    // 再生状態は、再読み込みの後の復元を確認できるよう、タブを閉じるまで保持する
    storage: sessionStorage
  });

  mockWindows('main');
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;

    switch (cmd) {
      case 'plugin:event|listen': {
        const event = args.event as string;
        const handlerId = args.handler as number;
        listeners.set(event, (listeners.get(event) ?? new Set()).add(handlerId));
        // 購読IDとしてコールバックIDを返す（unlisten時にeventIdとして渡される）
        return handlerId;
      }
      case 'plugin:event|unlisten':
        listeners.get(args.event as string)?.delete(args.eventId as number);
        return null;
      case 'plugin:event|emit':
        emit(args.event as string, args.payload);
        return null;
      case 'plugin:dialog|message': {
        const answer = answerMessageDialog(args.buttons, confirmResult);
        console.info(`[mock] ダイアログ「${String(args.message)}」→ ${answer}`);
        return answer;
      }
      case 'plugin:window|close':
        console.info('[mock] ウィンドウを閉じる操作は無視しました');
        return null;
      case 'plugin:app|version':
        // 実アプリと同じく、tauri.conf.jsonの版を返す（ADR-067）
        return tauriConfig.version;
    }

    if (cmd.startsWith('plugin:')) {
      console.warn(`[mock] 未対応のプラグインコマンドです: ${cmd}`, args);
      return null;
    }
    return backend.invoke(cmd, args);
  });

  // アルバムアート（albumartプロトコル）はモックバックエンドが生成した画像を返す
  // （アプリが`convertFileSrc`で作るURLは、アルバムアートだけ）
  tauriInternals().convertFileSrc = (trackId) =>
    backend.albumArtUrl(trackId) ?? MISSING_ALBUM_ART_URL;

  window.__MUSPICE_MOCK__ = {
    emit,
    setConfirmResult: (result) => {
      confirmResult = result;
    },
    setFolderResult: (path) => backend.setFolderResult(path),
    nowPlaying: () => backend.nowPlaying(),
    setM3uImportMode: (mode) => backend.setM3uImportMode(mode),
    setAlbumArtPickMode: (mode) => backend.setAlbumArtPickMode(mode),
    setFileImportMode: (mode) => backend.setFileImportMode(mode),
    openFiles: (fileNames) => backend.openFiles(fileNames),
    addDuplicateTracks: () => backend.addDuplicateTracks(),
    setConvertExportFolder: (path) => backend.setConvertExportFolder(path),
    setExclusionFolder: (path) => backend.setExclusionFolder(path),
    setMusicFolderFileCount: (count) => backend.setMusicFolderFileCount(count),
    setMusicFolderFreeBytes: (bytes) => backend.setMusicFolderFreeBytes(bytes)
  };

  console.info('[mock] Tauri IPCをモックしています（npm run dev:mock）');
}
