/**
 * 再生コントローラー
 *
 * Rust側の再生エンジン（`src-tauri/src/playback/`）をコマンドで操作し、エンジンからの通知
 * （`PlaybackEvent`）を再生状態（`./player.svelte.ts`）へ反映する。キューの遷移・再生回数の
 * 記録・イコライザの設定の送信・再生状態の復元と保存（`./playbackState.svelte.ts`）・OSのNow Playing
 * への通知（`./nowPlaying.ts`）もまとめて扱う。
 * Playerコンポーネントは表示と操作の受付だけを行い、再生の制御はここに委ねる。
 *
 * 再生キューはフロントエンド（`./player.svelte.ts`）が持つ。エンジンへは「再生する曲」と
 * 「続けて再生する曲」（`peekNextTrack`）だけを伝える。次・前のトラックの決定はキュー操作
 * （`playNextTrack` / `playPreviousTrack`）が担う。キュー操作は「再生中の曲をもう一度」の場合
 * （1曲リピート、3秒以上再生中の「前へ」、1曲だけのキューの全曲リピート）に同じトラックを
 * 再設定するが、トラックIDが変わらないためそれだけでは再生し直されない。ここでその場合を検出して
 * 頭から再生し直す。
 *
 * - 再生するトラック（`player.currentTrack`）が変わったら、`playbackPlay`で再生する
 * - ギャップレス再生かクロスフェードが有効なら、次の曲を`playbackSetNext`で伝えておく。エンジンは、
 *   再生中の曲の終わりから切れ目なく（クロスフェードでは、終わりと重ねて）続けて再生し、
 *   鳴っている曲が切り替わった時点で`advanced`を送る。ここでキューを進める
 * - 続けて再生する曲を伝えていなければ、曲の終わりで`ended`が届く。ここでキューを進め、次の曲を
 *   `playbackPlay`で再生する
 *
 * エンジンへ渡す番号（トークン）は、再生する曲ごとに振る。イベントにはその番号が付いて返るため、
 * 曲を切り替えた後に届いた前の曲のイベントを見分けて捨てられる。
 *
 * 音量の正規化とクロスフェードは、エンジンが設定を読んでかける（どの曲を重ねるか・重ねる長さも
 * エンジンが決める）。イコライザの設定はフロントエンドが保存しているため（`./equalizer.svelte.ts`）、
 * ここからエンジンへ送る。
 */
import { untrack } from 'svelte';
import { commands, events, type PlaybackEvent } from '#lib/bindings.js';
import { incrementPlayCount } from '#lib/queries/tracks.js';
import { EQ_FREQUENCIES, equalizer } from './equalizer.svelte.js';
import { handleError } from './error.svelte.js';
import { createNowPlayingReporter } from './nowPlaying.js';
import { restorePlaybackState, watchPlaybackState } from './playbackState.svelte.js';
import {
  peekNextTrack,
  player,
  playNextTrack,
  playPreviousTrack,
  resetPlayer
} from './player.svelte.js';
import type { Track } from '#lib/types/models.js';
import { m } from '#lib/i18n/i18n.svelte.js';

/** シークバーをドラッグしている間に、エンジンへシークを送る間隔（ms） */
const SCRUB_SEEK_INTERVAL_MS = 80;

export interface PlaybackControllerOptions {
  /**
   * ギャップレス再生（次の曲を先にエンジンへ伝え、曲の終わりで切れ目なく続ける）が有効かを返す
   * （リアクティブな値を読む）。省略時は無効
   */
  gapless?: () => boolean;
  /**
   * クロスフェードの秒数を返す（リアクティブな値を読む）。0または省略時はクロスフェードしない。
   * 1以上ならギャップレス再生の設定にかかわらず、次の曲を先にエンジンへ伝える
   * （重ねる処理は、エンジンが設定を読んで行う）
   */
  crossfadeSeconds?: () => number;
}

export interface PlaybackController {
  /** 再生/一時停止を切り替える */
  togglePlayPause(): Promise<void>;
  /** 指定位置（秒）へシークする */
  seek(time: number): void;
  /** 現在位置から相対的にシークする（秒） */
  seekBy(delta: number): void;
  /** 次のトラックへ進む */
  next(): void;
  /** 前のトラックへ戻る（3秒以上再生中なら再生中のトラックの頭へ） */
  previous(): void;
  /** シークバーをドラッグ中は、再生位置の通知でドラッグ位置を上書きしないようにする */
  setScrubbing(scrubbing: boolean): void;
  /** 再生を止め、通知の購読を解除して、再生状態をリセットする */
  destroy(): void;
}

/**
 * エンジンへ渡す番号。コントローラーを作り直しても重ならないよう、モジュールで数える
 * （エンジンは、番号の大きい要求を新しい要求として扱う）
 */
let lastToken = 0;

/**
 * 再生エンジンを制御する再生コントローラーを作成する
 *
 * 作成した時点から`player.currentTrack`を監視し、トラックが変わるたびに再生する。
 * 監視は`$effect`のため、状態の変更から少し遅れて（マイクロタスクで）反映される。
 */
export function createPlaybackController(
  options: PlaybackControllerOptions = {}
): PlaybackController {
  /** エンジンが再生している曲の番号 */
  let currentToken: number | null = null;
  /** エンジンに再生させた（させている）曲 */
  let engineTrackId: string | null = null;
  /** エンジンが曲を持っているか（再生中か一時停止中。再生の失敗・終了の後はfalse） */
  let loaded = false;
  /** `playbackPlay`の結果を待っている数（待っている間は、再生/一時停止の操作を受けない） */
  let starting = 0;
  /** 再生回数を記録した曲（同じ曲の再生し直しでは、もう一度記録しない） */
  let countedTrackId: string | null = null;
  /** 続けて再生する曲として、エンジンに伝えた曲 */
  let queuedNext: { token: number; trackId: string } | null = null;
  let scrubbing = false;
  /** ドラッグ中に、まだエンジンへ送っていないシークの位置 */
  let pendingSeek: number | null = null;
  let seekTimer: ReturnType<typeof setTimeout> | null = null;
  let destroyed = false;
  /** 再生状態の保存をやめる関数（復元が済んでから保存を始める） */
  let stopSaving: (() => void) | null = null;
  const nowPlaying = createNowPlayingReporter((update) => commands.setNowPlaying(update));

  /** 次の曲を、先にエンジンへ伝えるか（ギャップレス再生かクロスフェードが有効） */
  const isPreloadEnabled = () =>
    (options.gapless?.() ?? false) || (options.crossfadeSeconds?.() ?? 0) > 0;

  /** 再生の失敗を通知し、再生中の表示を解除する */
  function reportPlaybackError(error: unknown): void {
    handleError(error, m.errors.playbackFailed);
    player.isPlaying = false;
  }

  /** エンジンのコマンドを呼ぶ（結果を待たない操作用。失敗はログに残す） */
  function send(run: () => Promise<unknown>, description: string): void {
    run().catch((error) => console.error(`${description}に失敗しました:`, error));
  }

  /**
   * トラックを頭から再生する
   * @param countPlay falseなら、バックエンドへの通知と再生回数の記録をしない（同じ曲の再生し直し）
   */
  async function start(track: Track, countPlay = true): Promise<void> {
    const token = ++lastToken;
    currentToken = token;
    engineTrackId = track.id;
    queuedNext = null;
    loaded = false;
    cancelPendingSeek();
    player.currentTime = 0;
    // 曲の長さは、再生エンジンがファイルから読むまで、ライブラリの値を出しておく
    player.duration = track.duration ?? 0;
    starting++;

    try {
      const info = await commands.playbackPlay(track.id, token);
      // 結果を待つ間に別の曲へ切り替わっていたら、何もしない
      if (token !== currentToken || destroyed) return;

      loaded = true;
      player.duration = info.duration ?? track.duration ?? 0;
      player.isPlaying = true;
      syncNext();
      if (countPlay) await notifyStarted(track);
    } catch (error) {
      if (token !== currentToken || destroyed) return;
      reportPlaybackError(error);
    } finally {
      starting--;
    }
  }

  /** 現在再生中のトラックをバックエンドに通知し、再生回数を記録する */
  async function notifyStarted(track: Track): Promise<void> {
    countedTrackId = track.id;
    await commands.setCurrentTrack(track.id);
    void incrementPlayCount(track.id);
  }

  /**
   * 続けて再生する曲を、エンジンに伝える
   *
   * キュー・リピート・シャッフル・設定が変わるたびに呼ぶ。伝えてある曲と同じなら何もしない。
   */
  function syncNext(): void {
    if (!loaded) return;
    const next = isPreloadEnabled() ? peekNextTrack() : null;
    if ((next?.id ?? null) === (queuedNext?.trackId ?? null)) return;

    const token = ++lastToken;
    const queued = next ? { token, trackId: next.id } : null;
    queuedNext = queued;
    commands.playbackSetNext(next?.id ?? null, token).catch((error) => {
      // 用意に失敗しても、曲の終わりで通常の手順で再生する（そこでエラーを通知する）
      console.warn('次のトラックの準備に失敗しました:', error);
      if (queuedNext === queued) queuedNext = null;
    });
  }

  /** 再生中のトラックを頭から再生し直す */
  function restart(): void {
    const track = player.currentTrack;
    if (!track) return;
    if (!loaded) {
      void start(track, false);
      return;
    }
    cancelPendingSeek();
    player.currentTime = 0;
    send(() => commands.playbackSeek(0), 'シーク');
    if (!player.isPlaying) {
      player.isPlaying = true;
      send(() => commands.playbackResume(), '再生の再開');
    }
  }

  /**
   * キューを移動する
   *
   * 移動先が再生中と同じトラックだった場合は、`player.currentTrack`が変わらず再生が
   * 始まらないため、ここで頭から再生し直す。
   * @returns キューを移動できたか（ストアのキュー操作の戻り値）
   */
  function moveInQueue(move: () => boolean): boolean {
    const playingId = player.currentTrack?.id;
    const moved = move();
    if (moved && playingId !== undefined && player.currentTrack?.id === playingId) {
      restart();
    }
    return moved;
  }

  /** 曲が終わった後、キューの次の曲へ進む（次がなければ再生を終える） */
  function advanceQueueAfterEnd(): void {
    loaded = false;
    queuedNext = null;
    player.currentTime = 0;
    if (!moveInQueue(playNextTrack)) {
      resetPlayer();
    }
  }

  /** エンジンが、続けて再生する曲へ切れ目なく切り替わった */
  function onAdvanced(token: number, duration: number | null): void {
    const queued = queuedNext;
    if (queued?.token !== token || peekNextTrack()?.id !== queued.trackId) {
      // エンジンに伝えた後でキューが変わり、もうキューの次の曲ではない曲へ進んだ。
      // キューに合わせて再生し直す
      if (token !== currentToken) advanceQueueAfterEnd();
      return;
    }

    const repeating = queued.trackId === player.currentTrack?.id;
    currentToken = token;
    // 先に記録しておき、下の`playNextTrack`による`player.currentTrack`の変更で再生し直さない
    engineTrackId = queued.trackId;
    queuedNext = null;
    playNextTrack();

    const track = player.currentTrack;
    player.currentTime = 0;
    player.duration = duration ?? track?.duration ?? 0;
    syncNext();
    // 同じ曲の繰り返し（1曲リピート）では、通知・記録をしない（頭からの再生し直しと同じ）
    if (track && !repeating) {
      notifyStarted(track).catch((error) => console.error('再生の通知に失敗しました:', error));
    }
  }

  function onEvent(event: PlaybackEvent): void {
    if (destroyed) return;
    switch (event.type) {
      case 'position':
        if (event.token === currentToken && !scrubbing && event.position !== null) {
          player.currentTime = event.position;
        }
        break;
      case 'advanced':
        onAdvanced(event.token, event.duration);
        break;
      case 'ended':
        if (event.token === currentToken) advanceQueueAfterEnd();
        break;
      case 'failed':
        if (event.token !== currentToken && event.token !== queuedNext?.token) break;
        loaded = false;
        queuedNext = null;
        reportPlaybackError(event.error);
        break;
    }
  }

  // ---------- シーク ----------

  function cancelPendingSeek(): void {
    pendingSeek = null;
    if (seekTimer !== null) {
      clearTimeout(seekTimer);
      seekTimer = null;
    }
  }

  /** まだ送っていないシークがあれば、エンジンへ送る */
  function flushPendingSeek(): void {
    const position = pendingSeek;
    cancelPendingSeek();
    if (position !== null && loaded) {
      send(() => commands.playbackSeek(position), 'シーク');
    }
  }

  function seek(time: number): void {
    // エンジンが曲を持っていない間（起動して復元した直後・再生に失敗した後）は、頭から再生する
    // ことになるため、位置を動かさない
    if (!loaded) return;
    const max = player.duration > 0 ? player.duration : time;
    const clamped = Math.max(0, Math.min(time, max));
    player.currentTime = clamped;

    if (!scrubbing) {
      cancelPendingSeek();
      send(() => commands.playbackSeek(clamped), 'シーク');
      return;
    }
    // ドラッグ中はマウスが動くたびに呼ばれる。そのたびにシークすると音が細切れになるため、
    // 間隔を空けて、最後の位置だけを送る（ドラッグを終えた時点で、残りを送る）
    pendingSeek = clamped;
    seekTimer ??= setTimeout(() => {
      seekTimer = null;
      flushPendingSeek();
    }, SCRUB_SEEK_INTERVAL_MS);
  }

  /** 再生を止める（キューが空になったとき） */
  function stop(): void {
    if (currentToken === null && engineTrackId === null) return;
    currentToken = null;
    engineTrackId = null;
    queuedNext = null;
    loaded = false;
    cancelPendingSeek();
    send(() => commands.playbackStop(), '再生の停止');
  }

  // ---------- 前回の再生状態 ----------

  /**
   * 前回の終了時の再生状態（音量・キュー・再生していた曲）を復元し、その後の変更を保存し始める
   *
   * 復元した曲は、再生を始めない（再生ボタンで頭から再生する）。
   */
  async function restore(): Promise<void> {
    try {
      await restorePlaybackState({
        isCancelled: () => destroyed,
        // 復元した曲を、下の「再生するトラックの読み込み」の監視で再生し始めないよう、
        // エンジンに再生させた曲として記録しておく（`loaded`はfalseのまま）
        onTrackRestoring: (track) => {
          engineTrackId = track.id;
        }
      });
    } catch (error) {
      console.error('再生状態の復元に失敗しました:', error);
    }
    if (!destroyed) stopSaving = watchPlaybackState();
  }
  void restore();

  // ---------- エンジンからの通知 ----------

  const listening = events.playbackEvent.listen((event) => onEvent(event.payload));
  listening.catch((error) => console.error('再生エンジンの通知を購読できません:', error));

  // ---------- 再生状態の監視 ----------

  // コンポーネントの外でも動かし、destroy()で止めるため$effect.rootで作る
  const stopEffects = $effect.root(() => {
    // 再生するトラックの読み込み
    $effect(() => {
      const track = player.currentTrack;
      // 読み込みの処理の中で読む状態（再生中かどうか等）には反応させない
      untrack(() => {
        if (!track) {
          stop();
          return;
        }
        if (track.id === engineTrackId) return;
        void start(track);
      });
    });

    // 続けて再生する曲（キュー・リピート・シャッフル・設定の変更に追随する）
    $effect(() => {
      // 変更を検知するために、どちらも読む（何を伝えるかは`syncNext`が決める）
      peekNextTrack();
      isPreloadEnabled();
      untrack(syncNext);
    });

    $effect(() => {
      const volume = player.volume;
      send(() => commands.playbackSetVolume(volume), '音量の設定');
    });

    // イコライザ: 保存してある設定をエンジンへ送り、変更のたびに送り直す
    $effect(() => {
      const enabled = equalizer.enabled;
      const bands = equalizer.bands;
      const gains = EQ_FREQUENCIES.map((frequency) => bands[frequency]);
      send(() => commands.playbackSetEqualizer(enabled, gains), 'イコライザの設定');
    });

    // OSのNow Playing: プレーヤーバーと同じ内容を伝える（伝え直すかどうかは`nowPlaying`が決める）。
    // 起動して復元しただけの曲も、一時停止中として伝える（macOSは、一度も再生していないアプリを
    // 「再生中のアプリ」にしないため、ほかのアプリからメディアキーの対象を奪うことはない）
    $effect(() => {
      const track = player.currentTrack;
      nowPlaying.update(
        track && {
          trackId: track.id,
          playing: player.isPlaying,
          position: player.currentTime,
          duration: player.duration
        }
      );
    });
  });

  return {
    async togglePlayPause() {
      const track = player.currentTrack;
      if (!track || starting > 0) return;
      if (!loaded) {
        // 再生に失敗した後など: 頭から再生し直す
        await start(track, countedTrackId !== track.id);
        return;
      }
      try {
        if (player.isPlaying) {
          player.isPlaying = false;
          await commands.playbackPause();
        } else {
          player.isPlaying = true;
          await commands.playbackResume();
        }
      } catch (error) {
        reportPlaybackError(error);
      }
    },
    seek,
    seekBy(delta) {
      seek(player.currentTime + delta);
    },
    next() {
      moveInQueue(playNextTrack);
    },
    previous() {
      moveInQueue(playPreviousTrack);
    },
    setScrubbing(value) {
      scrubbing = value;
      if (!value) flushPendingSeek();
    },
    destroy() {
      destroyed = true;
      // 再生状態の保存は、下で再生状態をリセットする前にやめる（空のキューを保存しない）
      stopSaving?.();
      stopEffects();
      cancelPendingSeek();
      void listening.then((unlisten) => unlisten()).catch(() => {});
      currentToken = null;
      send(() => commands.playbackStop(), '再生の停止');
      nowPlaying.update(null);
      resetPlayer();
    }
  };
}
