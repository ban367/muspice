/**
 * トラック一覧の選択（クリック・キーボード）
 *
 * 一覧ごとに`TrackSelection`を1つ作り、行のクリックと一覧のキー操作をここへ渡す。
 * 選択中のトラックに加えて、キーボードで移動する時の現在位置（最後に選んだトラック）と
 * Shiftで範囲選択する時の起点を持つ。
 */

import { SvelteSet } from 'svelte/reactivity';
import { navigationTarget } from './listNavigation.js';
import { computeClickSelection, type SelectionModifiers } from './selection.js';

export class TrackSelection {
  /** 選択中のトラックID（挿入順は選択した順） */
  readonly #ids = new SvelteSet<string>();
  /** キーボードで移動する時の現在位置 */
  #activeId = $state<string | null>(null);
  /** Shiftで範囲選択する時の起点 */
  #anchorId: string | null = null;
  readonly #tracks: () => readonly { id: string }[];

  /**
   * @param tracks - 表示順のトラック一覧を返す関数（並び替え・絞り込みに追随させる）
   */
  constructor(tracks: () => readonly { id: string }[]) {
    this.#tracks = tracks;
  }

  /** 選択中のトラックID（読み取り用。変更はこのクラスのメソッドで行う） */
  get ids(): Set<string> {
    return this.#ids;
  }

  /** キーボードで移動する時の現在位置（最後に選んだトラック。選んでいなければnull） */
  get activeId(): string | null {
    return this.#activeId;
  }

  get size(): number {
    return this.#ids.size;
  }

  has(trackId: string): boolean {
    return this.#ids.has(trackId);
  }

  /** 選択中のトラックIDを、渡した順に入れ替える */
  #replace(trackIds: Iterable<string>): void {
    // 渡されたものが今の選択から作られていても空にならないよう、先に配列へ写す
    const next = [...trackIds];
    this.#ids.clear();
    for (const id of next) {
      this.#ids.add(id);
    }
  }

  /** クリックによる選択（修飾キーの扱いは`computeClickSelection`） */
  click(trackId: string, modifiers: SelectionModifiers): void {
    this.#replace(computeClickSelection(this.#ids, this.#tracks(), trackId, modifiers));
    this.#activeId = trackId;
    if (!modifiers.shiftKey) {
      this.#anchorId = trackId;
    }
  }

  /** そのトラックだけを選択する */
  selectOnly(trackId: string): void {
    this.#replace([trackId]);
    this.#activeId = trackId;
    this.#anchorId = trackId;
  }

  /** 選択中でなければ、そのトラックだけを選択する（コンテキストメニュー・ドラッグを始める時） */
  ensureSelected(trackId: string): void {
    if (!this.#ids.has(trackId)) {
      this.selectOnly(trackId);
    }
  }

  /**
   * ドラッグを始める時に、運ぶトラックIDを返す（表示順）
   *
   * 選択中のトラックの上で始めた場合は選択中のトラックすべて、そうでない場合は
   * そのトラックだけを選択して運ぶ。
   */
  beginDrag(trackId: string): string[] {
    this.ensureSelected(trackId);
    return this.#tracks()
      .filter((track) => this.#ids.has(track.id))
      .map((track) => track.id);
  }

  /** 一覧のトラックをすべて選択する */
  selectAll(): void {
    this.#replace(this.#tracks().map((track) => track.id));
  }

  /** 選択を解除する（現在位置は残し、続けてキーボードで移動できるようにする） */
  clear(): void {
    this.#ids.clear();
  }

  /** 選択と現在位置をどちらも消す（表示する一覧が別のものに変わった時） */
  reset(): void {
    this.#ids.clear();
    this.#activeId = null;
    this.#anchorId = null;
  }

  /**
   * キーボードで現在位置を移し、移動先を選択する
   *
   * - Shiftなし: 移動先だけを選択する
   * - Shiftあり: 範囲選択の起点から移動先までを選択する
   * @param key - 押したキー（`KeyboardEvent.key`）
   * @param extend - Shiftを押しているか
   * @param columns - グリッド表示の列数（リスト表示ではnull）
   * @returns 移動先のトラックID。扱わないキー・一覧が空の場合はnull
   */
  move(key: string, extend: boolean, columns: number | null = null): string | null {
    const tracks = this.#tracks();
    const indexOf = (id: string | null) =>
      id === null ? -1 : tracks.findIndex((track) => track.id === id);

    const current = indexOf(this.#activeId);
    const target = navigationTarget(key, current, tracks.length, columns);
    if (target === null) return null;

    const targetId = tracks[target].id;
    const anchor = extend ? indexOf(this.#anchorId ?? this.#activeId) : -1;
    if (anchor === -1) {
      this.#replace([targetId]);
      this.#anchorId = targetId;
    } else {
      // 移動先が「最後に選択したトラック」になるよう、起点から移動先へ向かう順で入れる
      const range = tracks.slice(Math.min(anchor, target), Math.max(anchor, target) + 1);
      const ordered = anchor <= target ? range : [...range].reverse();
      this.#replace(ordered.map((track) => track.id));
      this.#anchorId = tracks[anchor].id;
    }
    this.#activeId = targetId;
    return targetId;
  }
}

export interface TrackListKeyOptions {
  /** グリッド表示の列数を返す（リスト表示ではnull） */
  columns?: () => number | null;
  /** Enterで行う操作（現在位置のトラックを再生する、など） */
  onActivate?: (trackId: string) => void;
}

/**
 * トラック一覧のキーボード操作（一覧の要素の`onkeydown`から呼ぶ）
 *
 * - 矢印キー・Home・End: 選択を移す（Shiftで範囲選択）。移動先が見える位置までスクロールする
 * - Cmd/Ctrl+A: すべて選択する
 * - Enter: `onActivate`を呼ぶ
 *
 * 行の要素には`data-track-id`でトラックIDを付けておく。
 */
export function handleTrackListKeydown(
  event: KeyboardEvent,
  selection: TrackSelection,
  options: TrackListKeyOptions = {}
): void {
  if (event.defaultPrevented || event.altKey || event.isComposing) return;

  const target = event.target instanceof Element ? event.target : null;
  // 入力欄の中では、文字の入力・カーソルの移動を優先する
  if (target?.closest('input, textarea, select, [contenteditable="true"]')) return;

  if (event.metaKey || event.ctrlKey) {
    if (event.key.toLowerCase() === 'a' && !event.shiftKey) {
      selection.selectAll();
      event.preventDefault();
    }
    return;
  }

  if (event.key === 'Enter') {
    // 行の中のボタン（評価の星など）では、そのボタンの操作を優先する
    if (target?.closest('button, a')) return;
    if (selection.activeId !== null && options.onActivate) {
      options.onActivate(selection.activeId);
      event.preventDefault();
    }
    return;
  }

  const trackId = selection.move(event.key, event.shiftKey, options.columns?.() ?? null);
  if (trackId === null) return;

  // 既定の動作（一覧のスクロール）の代わりに、移動先の行を見える位置へ出す
  event.preventDefault();
  if (event.currentTarget instanceof Element) {
    event.currentTarget
      .querySelector(`[data-track-id="${CSS.escape(trackId)}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  }
}
