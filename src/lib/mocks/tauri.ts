/**
 * ブラウザ上でTauri IPCをモックする（`npm run dev:mock`専用）
 *
 * `@tauri-apps/api/mocks`で`window.__TAURI_INTERNALS__`を差し替え、
 * アプリのコマンドはインメモリバックエンドへ、プラグイン（event・dialog・window）は
 * ブラウザ内で完結する簡易実装へ振り分ける。状態はメモリ上にのみ保持し、
 * リロードするとフィクスチャの初期状態に戻る。
 *
 * ネイティブメニューは存在しないため、メニュー由来のイベントは開発者ツールから
 * `window.__MUSPICE_MOCK__.emit('toggle-sidebar')`のように発火させる。
 */
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { createMockBackend } from './backend';
import { createToneWav } from './media';

/** アートがないトラックに返すURL（読み込みエラーになり、実アプリの404と同じ扱いになる） */
const MISSING_ALBUM_ART_URL = 'data:image/png;base64,';

/** フォルダ選択ダイアログで選ばれたことにするパス（`setFolderResult`で切り替える） */
const MOCK_IMPORT_FOLDER = '/Users/demo/Music/Mock Import';

/** 開発者ツールから操作するためのハンドル */
interface MuspiceMockHandle {
  /** バックエンド（Rust）からのイベント送信を再現する */
  emit(event: string, payload?: unknown): void;
  /** 確認ダイアログ（dialogプラグインのconfirm）の回答を切り替える。既定はOK */
  setConfirmResult(result: boolean): void;
  /**
   * フォルダ選択ダイアログで選ばれたことにするパスを切り替える（nullはキャンセル）
   *
   * 既定のパスはライブラリフォルダの中のため、転送先デバイスの追加を確認するときは
   * ライブラリの外のパス（例: `/Volumes/NEW_SD`）にする。
   */
  setFolderResult(path: string | null): void;
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

export function setupTauriMock(): void {
  // イベントの購読はここで管理する。公式のshouldMockEventsはunlisten時に
  // 購読を解除しないため、解除済みのコールバックへ送信して警告が出てしまう
  const listeners = new Map<string, Set<number>>();
  let confirmResult = true;
  let folderResult: string | null = MOCK_IMPORT_FOLDER;

  function emit(event: string, payload?: unknown): void {
    for (const handlerId of [...(listeners.get(event) ?? [])]) {
      tauriInternals().runCallback(handlerId, { event, id: handlerId, payload });
    }
  }

  const backend = createMockBackend({ emit });

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
      case 'plugin:dialog|open': {
        const options = args.options as { directory?: boolean } | undefined;
        return options?.directory ? folderResult : null;
      }
      case 'plugin:dialog|message': {
        const answer = answerMessageDialog(args.buttons, confirmResult);
        console.info(`[mock] ダイアログ「${String(args.message)}」→ ${answer}`);
        return answer;
      }
      case 'plugin:window|close':
        console.info('[mock] ウィンドウを閉じる操作は無視しました');
        return null;
    }

    if (cmd.startsWith('plugin:')) {
      console.warn(`[mock] 未対応のプラグインコマンドです: ${cmd}`, args);
      return null;
    }
    return backend.invoke(cmd, args);
  });

  // 実ファイルの代わりに、パスごとに音程の異なる短いトーンを再生する
  // アルバムアート（albumartプロトコル）はモックバックエンドが生成した画像を返す
  const audioUrls = new Map<string, string>();
  tauriInternals().convertFileSrc = (filePath, protocol = 'asset') => {
    if (protocol === 'albumart') {
      return backend.albumArtUrl(filePath) ?? MISSING_ALBUM_ART_URL;
    }
    let url = audioUrls.get(filePath);
    if (!url) {
      url = URL.createObjectURL(new Blob([createToneWav(filePath)], { type: 'audio/wav' }));
      audioUrls.set(filePath, url);
    }
    return url;
  };

  window.__MUSPICE_MOCK__ = {
    emit,
    setConfirmResult: (result) => {
      confirmResult = result;
    },
    setFolderResult: (path) => {
      folderResult = path;
    }
  };

  console.info('[mock] Tauri IPCをモックしています（npm run dev:mock）');
}
