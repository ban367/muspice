/**
 * 再生コントローラー
 *
 * Rust側の再生エンジン（`src-tauri/src/playback/`）をコマンドで操作し、エンジンからの通知
 * （`PlaybackEvent`）を再生状態（`./player.svelte.ts`）へ反映する。キューの遷移・再生回数の
 * 記録（数えるかどうかは`./playTracker.ts`が聴いた時間で決める）・イコライザの設定の送信・
 * 再生状態の復元と保存（`./playbackState.svelte.ts`）・OSのNow Playingへの通知（`./nowPlaying.ts`）も
 * まとめて扱う。
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
 *
 * 再生の補助（`./playbackAids.svelte.ts`）もここで扱う。
 * - 「この曲が終わったら停止」が有効な間は、次の曲をエンジンへ伝えない。曲の終わり（`ended`）で、
 *   キューを次の曲へ進めるが、再生は始めない
 * - スリープタイマーは、時間が来たら、音量を少しずつ下げてから一時停止する（音量の設定は変えない）。
 *   「曲の終わりまで再生してから止める」場合は、時間が来た時点で「この曲が終わったら停止」にする
 */
import { untrack } from 'svelte';
import { commands, events, type PlaybackEvent } from '#lib/bindings.js';
import type { QueryClient } from '@tanstack/svelte-query';
import { recordPlay, recordSkip } from '#lib/queries/tracks.js';
import { EQ_FREQUENCIES, equalizer } from './equalizer.svelte.js';
import { handleError } from './error.svelte.js';
import { createNowPlayingReporter } from './nowPlaying.js';
import { createPlayTracker } from './playTracker.js';
import { playbackAids, SLEEP_FADE_SECONDS } from './playbackAids.svelte.js';
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

/** スリープタイマーの時間が来たかを確かめる間隔（ms。再生位置の通知でも確かめる） */
const SLEEP_CHECK_INTERVAL_MS = 1000;

/** スリープタイマーで音量を下げていく間、エンジンへ音量を送る間隔（ms） */
const SLEEP_FADE_STEP_MS = 200;

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
  /**
   * 再生回数・スキップ回数を記録した時に、キャッシュ（曲の一覧・再生履歴）へ反映する先。
   * 省略時は、記録だけを行う
   */
  queryClient?: QueryClient;
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

/** いま動いている再生コントローラー（プレーヤーが作る。なければnull） */
let activeController: PlaybackController | null = null;

/**
 * 再生位置を、指定した位置（秒）へ移す（プレーヤーの外の部品から呼ぶ。歌詞の行のクリックなど）
 *
 * 再生コントローラーがない・エンジンが曲を持っていない間は、何もしない。
 */
export function seekPlayback(time: number): void {
  activeController?.seek(time);
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
  /** 再生回数・スキップ回数に数えるかどうかの判定（聴いた時間で決める。`./playTracker.ts`） */
  const tracker = createPlayTracker({
    onPlayed: (trackId) => void recordPlay(trackId, options.queryClient),
    onSkipped: (trackId) => void recordSkip(trackId, options.queryClient)
  });
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
  /** スリープタイマーで音量を下げている間、音量をエンジンへ送るタイマー（下げていなければnull） */
  let fadeTimer: ReturnType<typeof setInterval> | null = null;

  /** 次の曲を、先にエンジンへ伝えるか（ギャップレス再生かクロスフェードが有効） */
  const isPreloadEnabled = () =>
    (options.gapless?.() ?? false) || (options.crossfadeSeconds?.() ?? 0) > 0;

  /** 再生の失敗を通知し、再生中の表示を解除する */
  function reportPlaybackError(error: unknown): void {
    handleError(error, m.errors.playbackFailed);
    player.isPlaying = false;
    tracker.reset();
  }

  /** エンジンのコマンドを呼ぶ（結果を待たない操作用。失敗はログに残す） */
  function send(run: () => Promise<unknown>, description: string): void {
    run().catch((error) => console.error(`${description}に失敗しました:`, error));
  }

  /**
   * トラックを頭から再生する
   *
   * 前の曲を再生回数に数える前に別の曲へ移ってきた場合は、前の曲がスキップに数えられる。
   */
  async function start(track: Track): Promise<void> {
    const token = ++lastToken;
    currentToken = token;
    engineTrackId = track.id;
    queuedNext = null;
    loaded = false;
    cancelPendingSeek();
    player.currentTime = 0;
    // 曲の長さは、再生エンジンがファイルから読むまで、ライブラリの値を出しておく
    player.duration = track.duration ?? 0;
    tracker.begin(track.id, player.duration);
    starting++;

    try {
      const info = await commands.playbackPlay(track.id, token);
      // 結果を待つ間に別の曲へ切り替わっていたら、何もしない
      if (token !== currentToken || destroyed) return;

      loaded = true;
      player.duration = info.duration ?? track.duration ?? 0;
      tracker.setDuration(player.duration);
      player.isPlaying = true;
      syncNext();
      await commands.setCurrentTrack(track.id);
    } catch (error) {
      if (token !== currentToken || destroyed) return;
      reportPlaybackError(error);
    } finally {
      starting--;
    }
  }

  /**
   * 続けて再生する曲を、エンジンに伝える
   *
   * キュー・リピート・シャッフル・設定が変わるたびに呼ぶ。伝えてある曲と同じなら何もしない。
   */
  function syncNext(): void {
    if (!loaded) return;
    // 「この曲が終わったら停止」の間は、次の曲を伝えない（曲の終わりで`ended`を受け取って止める）
    const next = isPreloadEnabled() && !playbackAids.stopAfterCurrent ? peekNextTrack() : null;
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
      void start(track);
      return;
    }
    cancelPendingSeek();
    player.currentTime = 0;
    // 頭から再生し直すため、聴いた時間を数え直す
    tracker.begin(track.id, player.duration);
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
    tracker.finish();
    loaded = false;
    queuedNext = null;
    player.currentTime = 0;
    if (playbackAids.stopAfterCurrent) {
      stopAfterTrackEnd();
      return;
    }
    if (!moveInQueue(playNextTrack)) {
      resetPlayer();
    }
  }

  /**
   * 「この曲が終わったら停止」: キューを次の曲へ進めるが、再生は始めない
   *
   * 止めた後は、指示を解除する。次の曲がある場合は、再生ボタンでその曲を頭から再生できる。
   * 次の曲がない場合は、通常の終わり方と同じにする。
   */
  function stopAfterTrackEnd(): void {
    playbackAids.stopAfterCurrent = false;
    cancelFade();
    player.isPlaying = false;
    // 伝えた後で指示が有効になり、エンジンがもう次の曲へ進んでいた場合に備えて、止めておく
    send(() => commands.playbackStop(), '再生の停止');

    const next = peekNextTrack();
    if (!next) {
      resetPlayer();
      return;
    }
    if (next.id !== player.currentTrack?.id) {
      // 下の`playNextTrack`による`player.currentTrack`の変更で、再生を始めないようにする
      engineTrackId = next.id;
      playNextTrack();
      player.duration = next.duration ?? 0;
      send(() => commands.setCurrentTrack(next.id), '再生中の曲の通知');
    }
  }

  // ---------- スリープタイマー ----------

  /** スリープタイマーの時間が来ていたら、再生を止める手順を始める */
  function checkSleepTimer(): void {
    const timer = playbackAids.sleepTimer;
    if (timer === null || fadeTimer !== null || Date.now() < timer.endsAt) return;

    // 再生していなければ、止めるものがない
    if (!loaded || !player.isPlaying) {
      playbackAids.cancelSleepTimer();
      return;
    }
    if (timer.waitForTrackEnd) {
      playbackAids.cancelSleepTimer();
      playbackAids.stopAfterCurrent = true;
      return;
    }
    startFade();
  }

  /** 音量を少しずつ下げ、下げ切ったら一時停止する（音量の設定`player.volume`は変えない） */
  function startFade(): void {
    playbackAids.isFadingOut = true;
    const startedAt = Date.now();
    const step = () => {
      const progress = (Date.now() - startedAt) / (SLEEP_FADE_SECONDS * 1000);
      if (progress < 1) {
        send(() => commands.playbackSetVolume(player.volume * (1 - progress)), '音量の設定');
      } else {
        finishFade();
      }
    };
    fadeTimer = setInterval(step, SLEEP_FADE_STEP_MS);
    step();
  }

  /** 音量を下げ切った: 一時停止して、スリープタイマーを終える */
  function finishFade(): void {
    const wasPlaying = loaded && player.isPlaying;
    if (!wasPlaying) {
      cancelFade();
      playbackAids.cancelSleepTimer();
      return;
    }
    player.isPlaying = false;
    // 一時停止してから、音量を設定の値に戻す（戻してから止めると、止まる直前に音が大きくなる）
    commands
      .playbackPause()
      .catch((error) => console.error('一時停止に失敗しました:', error))
      .finally(() => {
        cancelFade();
        playbackAids.cancelSleepTimer();
      });
  }

  /** 音量を下げるのをやめ、音量を設定の値に戻す（下げていなければ、何もしない） */
  function cancelFade(): void {
    if (fadeTimer === null) return;
    clearInterval(fadeTimer);
    fadeTimer = null;
    playbackAids.isFadingOut = false;
    send(() => commands.playbackSetVolume(player.volume), '音量の設定');
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
    // 前の曲は最後まで再生された
    tracker.finish();
    currentToken = token;
    // 先に記録しておき、下の`playNextTrack`による`player.currentTrack`の変更で再生し直さない
    engineTrackId = queued.trackId;
    queuedNext = null;
    playNextTrack();

    const track = player.currentTrack;
    player.currentTime = 0;
    player.duration = duration ?? track?.duration ?? 0;
    // 同じ曲の繰り返し（1曲リピート）も、聴いた時間を数え直す
    if (track) tracker.begin(track.id, player.duration);
    syncNext();
    // 同じ曲の繰り返しでは、再生中の曲は変わらないため通知しない
    if (track && !repeating) {
      send(() => commands.setCurrentTrack(track.id), '再生中の曲の通知');
    }
  }

  function onEvent(event: PlaybackEvent): void {
    if (destroyed) return;
    switch (event.type) {
      case 'position':
        if (event.token === currentToken && event.position !== null) {
          tracker.progress(event.position);
          if (!scrubbing) player.currentTime = event.position;
        }
        // ウィンドウが隠れている間はタイマーが間引かれるため、通知が届くたびにも確かめる
        checkSleepTimer();
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
    tracker.reset();
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

    // 続けて再生する曲（キュー・リピート・シャッフル・設定・「この曲が終わったら停止」の変更に追随する）
    $effect(() => {
      // 変更を検知するために、どれも読む（何を伝えるかは`syncNext`が決める）
      peekNextTrack();
      isPreloadEnabled();
      void playbackAids.stopAfterCurrent;
      untrack(syncNext);
    });

    $effect(() => {
      const volume = player.volume;
      // スリープタイマーで音量を下げている間は、下げる処理が次の間隔で送る
      if (fadeTimer !== null) return;
      send(() => commands.playbackSetVolume(volume), '音量の設定');
    });

    // スリープタイマー: 設定している間、時間が来たかを定期的に確かめる
    $effect(() => {
      if (playbackAids.sleepTimer === null) return;
      const timer = setInterval(checkSleepTimer, SLEEP_CHECK_INTERVAL_MS);
      return () => clearInterval(timer);
    });

    // スリープタイマーの解除（音量を下げている途中で解除されたら、音量を戻して再生を続ける）
    $effect(() => {
      if (!playbackAids.isFadingOut) untrack(cancelFade);
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

  const controller: PlaybackController = {
    async togglePlayPause() {
      const track = player.currentTrack;
      if (!track || starting > 0) return;
      if (!loaded) {
        // 起動して復元した曲・再生に失敗した後など: 頭から再生する
        await start(track);
        return;
      }
      try {
        if (player.isPlaying) {
          player.isPlaying = false;
          await commands.playbackPause();
          // スリープタイマーで音量を下げている途中に自分で止めた場合は、タイマーを終える
          // （一時停止した後で、音量を設定の値に戻す）
          if (fadeTimer !== null) {
            cancelFade();
            playbackAids.cancelSleepTimer();
          }
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
      if (fadeTimer !== null) clearInterval(fadeTimer);
      fadeTimer = null;
      playbackAids.reset();
      void listening.then((unlisten) => unlisten()).catch(() => {});
      currentToken = null;
      tracker.reset();
      send(() => commands.playbackStop(), '再生の停止');
      nowPlaying.update(null);
      resetPlayer();
      if (activeController === controller) activeController = null;
    }
  };
  activeController = controller;
  return controller;
}
