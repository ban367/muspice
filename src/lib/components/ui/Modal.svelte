<!--
  @component Modal
  ネイティブの<dialog>（showModal）で表示する共通モーダル。
  Escキーで閉じる操作・背面の操作の無効化（フォーカスもモーダル内に閉じ込める）・
  最前面への表示・閉じた後のフォーカスの復帰はブラウザが担う。背景のクリックでも閉じる。

  表示状態は呼び出し側が持つ。閉じる操作（Escキー・背景クリック・×ボタン）があると
  `onClose`が呼ばれるので、呼び出し側で`open`をfalseにする。

  開いたときは`data-autofocus`を付けた要素にフォーカスする（ない場合はブラウザの既定どおり
  最初のフォーカス可能な要素。ヘッダーがあれば×ボタン）。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** 表示するか */
    open: boolean;
    /** 閉じる操作があったときに呼ばれる */
    onClose: () => void;
    /** ヘッダーに表示するタイトル（省略時はヘッダーを表示しない） */
    title?: string;
    /** タイトルの前に表示するアイコン */
    titleIcon?: Snippet;
    /** タイトルがない場合のダイアログの名前（スクリーンリーダー向け） */
    label?: string;
    /** falseのとき閉じる操作を受け付けない（処理中など） */
    dismissible?: boolean;
    /** パネルに追加するクラス（幅の指定など） */
    class?: string;
    /** 本文を包む要素のクラス（独自のレイアウトにする場合に差し替える） */
    bodyClass?: string;
    /** 本文 */
    children: Snippet;
    /** フッター（ボタン類） */
    footer?: Snippet;
  }

  let {
    open,
    onClose,
    title,
    titleIcon,
    label,
    dismissible = true,
    class: className = 'max-w-xl',
    bodyClass = 'p-6 overflow-y-auto flex-1',
    children,
    footer
  }: Props = $props();

  const titleId = $props.id();

  let dialog = $state<HTMLDialogElement>();

  // 表示状態をdialog要素に反映する
  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) {
      dialog.showModal();
      dialog.querySelector<HTMLElement>('[data-autofocus]')?.focus();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  });

  function requestClose() {
    if (dismissible) onClose();
  }

  // Escキー: 閉じるかどうかは呼び出し側の状態で決めるため、ブラウザによる自動クローズは止める
  function handleCancel(event: Event) {
    event.preventDefault();
    requestClose();
  }

  // ブラウザがcancelを止めずに閉じた場合（連続したEscキーなど）も表示状態と食い違わないようにする
  function handleClose() {
    if (!open) return;
    if (dismissible) {
      onClose();
    } else {
      dialog?.showModal();
    }
  }

  /**
   * 背景（::backdrop）のクリックで閉じる（キーボードではEscキーが同じ役割を持つ）
   *
   * 背景のクリックはdialog要素自身へのクリックとして届く。パネル内で押して背景で離した
   * 場合（文字選択のドラッグなど）に閉じないよう、押した位置も背景だったときに限る。
   */
  function closeOnBackdropClick(node: HTMLDialogElement) {
    let pressedOnBackdrop = false;
    const handlePointerDown = (event: PointerEvent) => {
      pressedOnBackdrop = event.target === node;
    };
    const handleClick = (event: MouseEvent) => {
      if (pressedOnBackdrop && event.target === node) requestClose();
      pressedOnBackdrop = false;
    };
    node.addEventListener('pointerdown', handlePointerDown);
    node.addEventListener('click', handleClick);
    return () => {
      node.removeEventListener('pointerdown', handlePointerDown);
      node.removeEventListener('click', handleClick);
    };
  }
</script>

<dialog
  bind:this={dialog}
  class="modal-dialog"
  aria-labelledby={title ? titleId : undefined}
  aria-label={title ? undefined : label}
  oncancel={handleCancel}
  onclose={handleClose}
  {@attach closeOnBackdropClick}
>
  {#if open}
    <div class="modal-content {className}">
      {#if title}
        <div class="flex justify-between items-center p-6 border-b border-border">
          <h3
            id={titleId}
            class="m-0 text-xl font-semibold text-text-primary flex items-center gap-2"
          >
            {@render titleIcon?.()}
            {title}
          </h3>
          <button
            class="btn-icon w-8 h-8"
            onclick={requestClose}
            disabled={!dismissible}
            aria-label="閉じる"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              class="w-5 h-5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </div>
      {/if}

      <div class={bodyClass}>
        {@render children()}
      </div>

      {#if footer}
        <div class="flex justify-end gap-3 p-6 border-t border-border">
          {@render footer()}
        </div>
      {/if}
    </div>
  {/if}
</dialog>

<style>
  @reference "../../../app.css";

  /* dialog要素を画面全体に広げ、その中央上寄りにパネルを置く（余白部分のクリックが背景のクリックになる） */
  .modal-dialog {
    @apply fixed inset-0 m-0 h-full w-full max-h-none max-w-none border-0 bg-transparent;
    padding: 4rem 2rem calc(4rem + var(--spacing-player-height)) 2rem;
  }

  .modal-dialog[open] {
    @apply flex items-start justify-center;
  }

  .modal-dialog::backdrop {
    @apply bg-black/80;
  }
</style>
