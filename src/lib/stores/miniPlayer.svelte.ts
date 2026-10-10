/**
 * ミニプレーヤー（メインウィンドウの小さな表示）の状態
 *
 * ウィンドウの大きさはRust側（`set_mini_player`）が変え、画面はここを見て切り替える。
 * 「常に手前に表示」はlocalStorageに保存する。ミニプレーヤーかどうかは保存しない
 * （起動時は、いつも通常の表示で始まる）。
 */
import { commands } from '#lib/bindings.js';
import { handleError } from './error.svelte.js';
import { m } from '#lib/i18n/i18n.svelte.js';

const ALWAYS_ON_TOP_KEY = 'muspice:miniPlayerAlwaysOnTop';

function loadAlwaysOnTop(): boolean {
  try {
    // 既定は、手前に表示する（ほかのアプリを使いながら見るための表示のため）
    return localStorage.getItem(ALWAYS_ON_TOP_KEY) !== 'false';
  } catch {
    return true;
  }
}

class MiniPlayerState {
  #isActive = $state(false);
  #alwaysOnTop = $state(loadAlwaysOnTop());
  /** ウィンドウの切り替えを待っているか（待っている間は、続けて切り替えない） */
  #isSwitching = false;

  /** ミニプレーヤーを表示しているか */
  get isActive(): boolean {
    return this.#isActive;
  }

  /** ミニプレーヤーの間、ウィンドウを常に手前に表示するか（localStorageに保存） */
  get alwaysOnTop(): boolean {
    return this.#alwaysOnTop;
  }

  /** ミニプレーヤーと通常の表示を切り替える */
  async toggle(): Promise<void> {
    await this.#switch(!this.#isActive);
  }

  /** 通常の表示に戻す */
  async exit(): Promise<void> {
    await this.#switch(false);
  }

  async #switch(active: boolean): Promise<void> {
    if (this.#isSwitching || active === this.#isActive) return;
    this.#isSwitching = true;
    try {
      await commands.setMiniPlayer(active, this.#alwaysOnTop);
      this.#isActive = active;
    } catch (error) {
      handleError(error, m.miniPlayer.switchFailed);
    } finally {
      this.#isSwitching = false;
    }
  }

  /** 「常に手前に表示」を切り替える（ミニプレーヤーの間なら、すぐに反映する） */
  async setAlwaysOnTop(alwaysOnTop: boolean): Promise<void> {
    this.#alwaysOnTop = alwaysOnTop;
    try {
      localStorage.setItem(ALWAYS_ON_TOP_KEY, String(alwaysOnTop));
    } catch {
      // 保存できなくても動作には影響しない
    }
    if (!this.#isActive) return;
    try {
      await commands.setMiniPlayer(true, alwaysOnTop);
    } catch (error) {
      handleError(error, m.miniPlayer.switchFailed);
    }
  }
}

export const miniPlayer = new MiniPlayerState();
