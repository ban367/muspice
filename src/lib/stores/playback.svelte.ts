/**
 * 再生コントローラー
 *
 * audio要素の操作（読み込み・再生・シーク・音量）と再生状態（`./player.svelte.ts`）の同期、
 * トラック終了時のキュー遷移、再生回数の記録、イコライザの接続をまとめて扱う。
 * Playerコンポーネントは表示と操作の受付だけを行い、再生の制御はここに委ねる。
 *
 * 次・前のトラックの決定はキュー操作（`playNextTrack` / `playPreviousTrack`）が担う。
 * キュー操作は「再生中の曲をもう一度」の場合（1曲リピート、3秒以上再生中の「前へ」、
 * 1曲だけのキューの全曲リピート）に同じトラックを再設定するが、トラックIDが変わらないため
 * それだけでは再生し直されない。ここでその場合を検出して頭から再生し直す。
 *
 * ギャップレス再生のため、audio要素（デッキ）を2つ使う。再生中のデッキとは別のデッキに
 * 次の曲（`peekNextTrack`）を先読みしておき、曲の終わりの直前に再生を始めて切り替える。
 * 先読みした曲がキューの操作で変わった場合は読み込み直し、切り替えの時点で次の曲と
 * 一致しない場合は、従来どおり曲の終わりで読み込んで再生する。
 *
 * クロスフェードも同じ仕組みで、曲の終わりの設定した秒数前に次の曲を再生し始め、
 * 2つのデッキの音量を交差させる（Web Audioの経路のデッキごとのフェード）。
 */
import { untrack } from 'svelte';
import { convertFileSrc } from '@tauri-apps/api/core';
import { commands } from '#lib/bindings.js';
import { incrementPlayCount } from '#lib/queries/tracks.js';
import { handleError } from './error.svelte.js';
import {
  cleanupEqualizer,
  fadeDeck,
  initializeEqualizer,
  isEqualizerInitialized,
  resumeAudioContext,
  setDeckFade,
  setNormalizationGain
} from './equalizer.svelte.js';
import {
  peekNextTrack,
  player,
  playNextTrack,
  playPreviousTrack,
  resetPlayer
} from './player.svelte.js';
import type { Track, VolumeNormalization } from '#lib/types/models.js';
import { normalizationGain } from '#lib/utils/normalization.js';

/** `MediaError.code`の値（Node環境のテストでも参照できるよう定数で持つ） */
const MEDIA_ERR_ABORTED = 1;
const MEDIA_ERR_NETWORK = 2;
const MEDIA_ERR_DECODE = 3;
const MEDIA_ERR_SRC_NOT_SUPPORTED = 4;

/**
 * 切り替えを始める時点の何秒前から、次の曲へ切り替えるタイマーを用意するか
 * （再生位置の通知（timeupdate）の間隔より長くする）
 */
const TRANSITION_WINDOW_SECONDS = 1;

/**
 * ギャップレス再生で、次の曲を曲の終わりの何秒前に再生し始めるか
 *
 * audio要素は再生を始めてから音が出るまでに少し遅れるため、その分だけ早める。
 * 早すぎると曲の終わりと重なり、遅すぎると無音が入る。
 */
const GAPLESS_LEAD_SECONDS = 0.03;

/** タイマーの誤差として許容する秒数（これより早く発火した場合は待ち直す） */
const TIMER_TOLERANCE_SECONDS = 0.01;

export interface PlaybackControllerOptions {
  /**
   * 音量の正規化の設定を返す（設定のクエリなど、リアクティブな値を読む）。
   * 値が変わると、再生中の曲の補正量も変わる。省略時は補正しない
   */
  normalizationMode?: () => VolumeNormalization;
  /**
   * ギャップレス再生（次の曲を先読みし、曲の終わりで切れ目なく続ける）が有効かを返す
   * （リアクティブな値を読む）。省略時は無効
   */
  gapless?: () => boolean;
  /**
   * クロスフェードの秒数を返す（リアクティブな値を読む）。0または省略時はクロスフェードしない。
   * 1以上ならギャップレス再生の設定にかかわらず次の曲を先読みする
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
  /** イベント購読とイコライザを解放し、再生状態をリセットする */
  destroy(): void;
}

/**
 * audio要素のエラーをユーザー向けメッセージにする
 * @returns 表示不要なエラー（再生の中断）の場合はnull
 */
export function mediaErrorMessage(error: Pick<MediaError, 'code'> | null): string | null {
  switch (error?.code) {
    case MEDIA_ERR_ABORTED:
      return null;
    case MEDIA_ERR_NETWORK:
      return 'ネットワークエラーが発生しました';
    case MEDIA_ERR_DECODE:
      return 'デコードエラー: ファイルが破損しているか未対応の形式です';
    case MEDIA_ERR_SRC_NOT_SUPPORTED:
      return '未対応のフォーマットか、ファイルが見つかりません';
    default:
      return '再生エラーが発生しました';
  }
}

/** トラック切り替えなどで再生が中断されたことによる拒否か */
function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError';
}

/** 再生に使うaudio要素（デッキ）と、読み込んだトラック */
interface Deck {
  /** デッキの番号（イコライザの経路で、音量の正規化をかける位置） */
  readonly index: number;
  readonly audio: HTMLAudioElement;
  /** 読み込んだ（読み込み中の）トラック */
  track: Track | null;
  /** トラックのファイルを`src`に設定し終えたか */
  ready: boolean;
}

/**
 * audio要素を制御する再生コントローラーを作成する
 *
 * 作成した時点から`player.currentTrack`を監視し、トラックが変わるたびに読み込んで再生する。
 * 監視は`$effect`のため、状態の変更から少し遅れて（マイクロタスクで）反映される。
 * 同じ同期処理の中でトラックが続けて変わった場合は、最後のトラックだけを読み込む。
 *
 * @param audios 再生に使う2つのaudio要素（デッキ）。ギャップレス再生が無効の間は1つ目だけを使う
 */
export function createPlaybackController(
  audios: readonly [HTMLAudioElement, HTMLAudioElement],
  options: PlaybackControllerOptions = {}
): PlaybackController {
  const decks: Deck[] = audios.map((audio, index) => {
    // 先読みしたデッキへすぐに切り替えられるよう、ファイル全体を読み込ませる
    audio.preload = 'auto';
    return { index, audio, track: null, ready: false };
  });
  /** 再生中のトラック（`player.currentTrack`）を受け持つデッキ */
  let active = decks[0];
  /** 次のトラックを先読みしておくデッキ */
  let standby = decks[1];
  let scrubbing = false;
  /** 次の曲へ切り替えるタイマー */
  let transitionTimer: ReturnType<typeof setTimeout> | null = null;
  /** 読み込みの世代（パスの取得中に別の読み込み・切り替えが始まったら、古い方を捨てる） */
  let loadGeneration = 0;
  /** 先読みの世代（同上） */
  let preloadGeneration = 0;
  /** クロスフェードでフェードアウト中の（前の曲の）デッキ */
  let fadingDeck: Deck | null = null;

  const normalizationMode = () => options.normalizationMode?.() ?? 'off';
  const crossfadeSetting = () => options.crossfadeSeconds?.() ?? 0;
  /** 次の曲を先読みするか（ギャップレス再生かクロスフェードが有効） */
  const isPreloadEnabled = () => (options.gapless?.() ?? false) || crossfadeSetting() > 0;

  /** 先読みしておくトラック（先読みが無効、または次の曲がなければnull） */
  const trackToPreload = () => (isPreloadEnabled() ? peekNextTrack() : null);

  /** デッキの音量の正規化の倍率を、読み込んだトラックと設定から決める */
  function applyNormalization(deck: Deck, mode = normalizationMode()): void {
    const replayGain = deck.track?.replayGain;
    setNormalizationGain(deck.index, replayGain ? normalizationGain(replayGain, mode) : 1);
  }

  /** 再生を開始する（中断による拒否は無視し、それ以外はログに残す） */
  async function play(): Promise<void> {
    try {
      await active.audio.play();
    } catch (error) {
      if (!isAbortError(error)) {
        console.error('再生の開始に失敗しました:', error);
      }
    }
  }

  /** トラックの再生の失敗を通知し、再生中の表示を解除する */
  function reportPlaybackError(error: unknown): void {
    handleError(error, 'トラックの再生に失敗しました');
    player.isPlaying = false;
  }

  /**
   * 読み込んだトラックの再生を始め、バックエンドへの通知と再生回数の記録を行う
   * @param notify falseなら通知・記録をしない（同じ曲を繰り返す場合。従来の頭からの再生し直しと同じ）
   */
  async function startPlayback(deck: Deck, track: Track, notify = true): Promise<void> {
    // ユーザー操作の後にAudioContextを再開する（自動再生ポリシー対応）
    await resumeAudioContext();

    try {
      await deck.audio.play();
    } catch (error) {
      if (isAbortError(error)) return;
      throw error;
    }
    if (!notify) return;

    // 現在再生中のトラックをバックエンドに通知し、再生回数を記録する
    await commands.setCurrentTrack(track.id);
    void incrementPlayCount(track.id);
  }

  /** トラックを再生中のデッキに読み込んで再生する */
  async function load(track: Track): Promise<void> {
    const deck = active;
    const generation = ++loadGeneration;
    cancelTransition();
    endCrossfade();
    // 前のトラックは切り替えを始めた時点で止める
    deck.audio.pause();
    setDeckFade(deck.index, 1);
    deck.track = track;
    deck.ready = false;
    applyNormalization(deck);

    try {
      const filePath = await commands.getTrackFilePath(track.id);

      // パスの取得中に別のトラックへ切り替わっていたら何もしない
      if (generation !== loadGeneration) return;

      deck.audio.src = convertFileSrc(filePath);
      deck.ready = true;
      await startPlayback(deck, track);
    } catch (error) {
      reportPlaybackError(error);
    }
  }

  /**
   * 次のトラックを、再生中ではない方のデッキに先読みする
   *
   * すでに同じトラックを読み込んでいれば何もしない。nullなら先読みを解除する。
   */
  async function preload(track: Track | null): Promise<void> {
    const deck = standby;
    // 切り替えの直後で前の曲の終わりがまだ鳴っている間は触らない（鳴り終わったら呼び直される）
    if (!deck.audio.paused) return;

    if (!track) {
      unload(deck);
      return;
    }
    if (deck.track?.id === track.id) {
      // 前に再生した位置が残っていれば（1曲リピートなどで同じ曲を使い回す場合）頭へ戻しておく
      if (deck.ready && deck.audio.currentTime !== 0) deck.audio.currentTime = 0;
      return;
    }

    const generation = ++preloadGeneration;
    deck.track = track;
    deck.ready = false;
    applyNormalization(deck);

    try {
      const filePath = await commands.getTrackFilePath(track.id);
      if (generation !== preloadGeneration || deck !== standby) return;
      deck.audio.src = convertFileSrc(filePath);
      deck.ready = true;
    } catch (error) {
      // 先読みに失敗しても、曲の切り替えのときに通常の読み込みで再試行する（そこでエラーを通知する）
      if (generation === preloadGeneration && deck === standby) deck.track = null;
      console.warn('次のトラックの先読みに失敗しました:', error);
    }
  }

  /** デッキの読み込みを解除する（ファイルを開いたままにしない） */
  function unload(deck: Deck): void {
    if (deck.track === null && !deck.ready) return;
    preloadGeneration++;
    deck.track = null;
    deck.ready = false;
    deck.audio.pause();
    deck.audio.removeAttribute('src');
    deck.audio.load();
  }

  /**
   * 先読みしたデッキを再生中のデッキにする（再生はしない）
   * @returns それまで再生中だったデッキ
   */
  function swapDecks(): Deck {
    cancelTransition();
    // 読み込み中のトラックがあれば捨てる
    loadGeneration++;
    const previous = active;
    active = standby;
    standby = previous;
    if (active.audio.currentTime !== 0) active.audio.currentTime = 0;
    player.currentTime = 0;
    player.duration = Number.isFinite(active.audio.duration) ? active.audio.duration : 0;
    return previous;
  }

  /** 先読みしたデッキが、次に再生するトラックを読み込み済みか */
  function isNextTrackReady(next: Track | null): next is Track {
    return next !== null && standby.ready && standby.track?.id === next.id;
  }

  /**
   * 次の曲へのクロスフェードの秒数（クロスフェードしない場合は0）
   *
   * 同じ曲の繰り返し（1曲リピート）はクロスフェードせず、切れ目なく続ける。
   * 短い曲では、どちらの曲も半分を超えて重ねない。
   * フェードはWeb Audioの経路でかけるため、経路を作れていない場合はクロスフェードしない。
   */
  function crossfadeSeconds(next: Track): number {
    const setting = crossfadeSetting();
    if (setting <= 0 || next.id === player.currentTrack?.id || !isEqualizerInitialized()) {
      return 0;
    }
    const halves = [active.audio.duration, standby.audio.duration]
      .filter((duration) => Number.isFinite(duration) && duration > 0)
      .map((duration) => duration / 2);
    return Math.min(setting, ...halves);
  }

  /** 次の曲を、曲の終わりの何秒前に再生し始めるか */
  function transitionLead(next: Track): number {
    return Math.max(crossfadeSeconds(next), GAPLESS_LEAD_SECONDS);
  }

  /**
   * 先読みした次の曲へ切り替える（ギャップレス再生・クロスフェード）
   *
   * 前の曲はそのまま最後まで鳴らす（ギャップレス再生では、少し早めに切り替えた分の曲の終わりを
   * 切らない。クロスフェードでは、フェードアウトさせながら鳴らす）。
   * @returns 切り替えたか（先読みが済んでいない・次の曲が変わった場合はfalse）
   */
  function startTransition(): boolean {
    cancelTransition();
    const next = peekNextTrack();
    if (!isPreloadEnabled() || !isNextTrackReady(next)) return false;

    const repeating = next.id === player.currentTrack?.id;
    // 切り替える前の（再生中の曲の長さで）秒数を決める
    const fade = crossfadeSeconds(next);
    endCrossfade();
    const previous = swapDecks();
    // 再生中のトラックを進める（先読みしたデッキと同じトラックのため、読み込みは走らない）
    playNextTrack();
    if (fade > 0 && !previous.audio.paused) {
      fadingDeck = previous;
      fadeDeck(previous.index, 'out', fade);
      fadeDeck(active.index, 'in', fade);
    } else {
      setDeckFade(active.index, 1);
    }
    startPlayback(active, next, !repeating).catch(reportPlaybackError);
    if (previous.audio.paused) {
      // 前の曲が鳴り終わっていれば（`ended`での切り替え）、すぐに次を用意する。
      // 1曲リピートではキューの状態が変わらず先読みの$effectが動かないため、ここで頭へ戻しておく
      void preload(trackToPreload());
    }
    return true;
  }

  /** 再生中の曲の残り秒数（長さが分からない場合はnull） */
  function remainingSeconds(): number | null {
    const { duration, currentTime } = active.audio;
    return Number.isFinite(duration) && duration > 0 ? duration - currentTime : null;
  }

  /** 曲の終わりが近づいたら、次の曲へ切り替えるタイマーを用意する */
  function scheduleTransition(): void {
    if (transitionTimer !== null || active.audio.paused || !isPreloadEnabled()) return;
    const next = peekNextTrack();
    if (!isNextTrackReady(next)) return;
    const remaining = remainingSeconds();
    const lead = transitionLead(next);
    if (remaining === null || remaining > lead + TRANSITION_WINDOW_SECONDS) return;
    waitForTransition(remaining - lead);
  }

  /** 指定した秒数後に、切り替えるかを確かめる */
  function waitForTransition(seconds: number): void {
    transitionTimer = setTimeout(onTransitionTimer, Math.max(0, seconds * 1000));
  }

  function onTransitionTimer(): void {
    transitionTimer = null;
    if (active.audio.paused) return;
    const next = peekNextTrack();
    const remaining = remainingSeconds();
    if (remaining === null || !isNextTrackReady(next)) return;
    // タイマーの誤差などでまだ早ければ待ち直す（設定や曲の長さが変わった場合も、ここで決め直す）
    const lead = transitionLead(next);
    if (remaining > lead + TIMER_TOLERANCE_SECONDS) {
      waitForTransition(remaining - lead);
      return;
    }
    startTransition();
  }

  function cancelTransition(): void {
    if (transitionTimer !== null) {
      clearTimeout(transitionTimer);
      transitionTimer = null;
    }
  }

  /**
   * クロスフェード中なら終わらせる（前の曲を止め、再生中の曲を通常の音量にする）
   *
   * クロスフェードの途中で一時停止・シーク・曲の選択などの操作をした場合に呼ぶ。
   */
  function endCrossfade(): void {
    if (fadingDeck === null) return;
    const deck = fadingDeck;
    fadingDeck = null;
    if (deck !== active) deck.audio.pause();
    setDeckFade(active.index, 1);
  }

  /** 再生中のトラックを頭から再生し直す */
  function restart(): void {
    cancelTransition();
    endCrossfade();
    active.audio.currentTime = 0;
    player.currentTime = 0;
    void play();
  }

  /**
   * キューを移動する
   *
   * 移動先が再生中と同じトラックだった場合は、トラックIDが変わらず読み込みが
   * 走らないため、ここで頭から再生し直す。
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

  function seek(time: number): void {
    const audio = active.audio;
    const max = Number.isFinite(audio.duration) ? audio.duration : time;
    const clamped = Math.max(0, Math.min(time, max));
    // 切り替えのタイマーは、次の再生位置の通知で用意し直す
    cancelTransition();
    endCrossfade();
    audio.currentTime = clamped;
    player.currentTime = clamped;
  }

  /** 再生をすべて止める（キューが空になったとき） */
  function stop(): void {
    cancelTransition();
    endCrossfade();
    loadGeneration++;
    active.track = null;
    active.ready = false;
    for (const deck of decks) {
      deck.audio.pause();
    }
  }

  // ---------- audio要素のイベント ----------
  // 2つのデッキのイベントを受け、再生中のデッキのものだけを再生状態に反映する

  const handlers: Array<[keyof HTMLMediaElementEventMap, (deck: Deck) => void]> = [
    [
      'play',
      (deck) => {
        if (deck === active) player.isPlaying = true;
      }
    ],
    [
      'pause',
      (deck) => {
        if (deck === active) {
          player.isPlaying = false;
          cancelTransition();
        } else {
          // 前の曲が鳴り終わった（止められた）ので、次の曲を先読みできる
          if (deck === fadingDeck) fadingDeck = null;
          void preload(trackToPreload());
        }
      }
    ],
    [
      'timeupdate',
      (deck) => {
        if (deck !== active) return;
        if (!scrubbing) player.currentTime = deck.audio.currentTime;
        scheduleTransition();
      }
    ],
    [
      'loadedmetadata',
      (deck) => {
        if (deck === active) player.duration = deck.audio.duration;
      }
    ],
    [
      'ended',
      (deck) => {
        if (deck !== active) {
          if (deck === fadingDeck) fadingDeck = null;
          void preload(trackToPreload());
          return;
        }
        player.currentTime = 0;
        // 先読みが済んでいれば、そのデッキで続けて再生する
        if (startTransition()) return;
        // 次がなければ（リピートなしでキューの最後）再生を終える
        if (!moveInQueue(playNextTrack)) {
          resetPlayer();
        }
      }
    ],
    [
      'error',
      (deck) => {
        const message = mediaErrorMessage(deck.audio.error);
        if (message === null) return;
        if (deck !== active) {
          // 先読みの失敗は、曲の切り替えのときに通常の読み込みで再試行する（そこでエラーを通知する）
          console.warn('次のトラックの先読みに失敗しました:', deck.audio.error);
          deck.track = null;
          deck.ready = false;
          return;
        }
        console.error('オーディオの再生エラーが発生しました', {
          error: deck.audio.error,
          src: deck.audio.src
        });
        handleError(message);
        player.isPlaying = false;
      }
    ]
  ];
  const listeners = decks.flatMap((deck) =>
    handlers.map(([type, handler]) => {
      const listener = () => handler(deck);
      deck.audio.addEventListener(type, listener);
      return { deck, type, listener };
    })
  );

  // ---------- 再生状態の監視 ----------

  // コンポーネントの外でも動かし、destroy()で止めるため$effect.rootで作る
  const stopEffects = $effect.root(() => {
    // 再生するトラックの読み込み（先読みより先に処理するため、先に作る）
    $effect(() => {
      const track = player.currentTrack;
      // 読み込みの処理の中で読む状態（再生中かどうか等）には反応させない
      untrack(() => {
        if (!track) {
          // キューが空になったら再生を止める
          stop();
          return;
        }
        if (track.id === active.track?.id) return;

        if (standby.ready && standby.track?.id === track.id) {
          // 先読みした曲へ「次へ」などで移った場合は、読み込み直さずに切り替える
          // （手動の操作ではクロスフェードしない）
          endCrossfade();
          const previous = swapDecks();
          previous.audio.pause();
          setDeckFade(active.index, 1);
          startPlayback(active, track).catch(reportPlaybackError);
          return;
        }
        void load(track);
      });
    });

    // 次の曲の先読み（キュー・リピート・シャッフル・設定の変更に追随する）
    $effect(() => {
      const next = trackToPreload();
      untrack(() => void preload(next));
    });

    $effect(() => {
      const volume = player.volume;
      for (const deck of decks) {
        deck.audio.volume = volume;
      }
    });

    // 音量の正規化: 設定が変わったら、各デッキのトラックの補正量を決め直す
    $effect(() => {
      const mode = normalizationMode();
      untrack(() => {
        for (const deck of decks) {
          applyNormalization(deck, mode);
        }
      });
    });
  });

  if (!isEqualizerInitialized()) {
    initializeEqualizer(audios).catch((error) => {
      console.error('イコライザの初期化に失敗しました:', error);
    });
  }

  return {
    async togglePlayPause() {
      if (!player.currentTrack) return;
      if (active.audio.paused) {
        await play();
      } else {
        // クロスフェードの途中なら、前の曲は止めて再開しない
        endCrossfade();
        active.audio.pause();
      }
    },
    seek,
    seekBy(delta) {
      seek(active.audio.currentTime + delta);
    },
    next() {
      moveInQueue(playNextTrack);
    },
    previous() {
      moveInQueue(playPreviousTrack);
    },
    setScrubbing(value) {
      scrubbing = value;
    },
    destroy() {
      stopEffects();
      cancelTransition();
      fadingDeck = null;
      loadGeneration++;
      preloadGeneration++;
      for (const { deck, type, listener } of listeners) {
        deck.audio.removeEventListener(type, listener);
      }
      resetPlayer();
      cleanupEqualizer().catch((error) => {
        console.error('イコライザのクリーンアップに失敗しました:', error);
      });
    }
  };
}
