<!--
  @component FileDropZone
  OS（Finderなど）からウィンドウへドロップされた音楽ファイル・フォルダを、インポートする。

  ドラッグしている間は、ウィンドウ全体に「ここへドロップ」の表示を重ねる。ドロップされたら、
  名前だけをRust側へ伝える（パスは、Rust側がOSから読む。`importer.importDropped`）。
  アプリの中のドラッグ（曲の並べ替えなど）は扱わない。

  ファイルのドロップを扱わないと、WebViewがそのファイルを開いてしまう（アプリの画面が
  ファイルの表示に置き換わる）ため、インポートできない場合もドロップそのものは受け取る。
-->
<script lang="ts">
  import { importer } from '#lib/stores/importer.svelte.js';
  import { droppedNames, isFileDrag } from '#lib/utils/fileDrop.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  /** ページの中で始まったドラッグの途中か（曲・画像など。OSからのドラッグと区別する） */
  let isInternalDrag = false;
  /** ファイルをドラッグして、ウィンドウの上にいるか */
  let isOver = $state(false);
  /** ドラッグがウィンドウの外へ出たとみなすまでの待ち（`dragover`が届かなくなってから） */
  let leaveTimer: ReturnType<typeof setTimeout> | null = null;

  const LEAVE_DELAY_MS = 150;

  function isExternalFileDrag(event: DragEvent): boolean {
    return !isInternalDrag && isFileDrag(event.dataTransfer);
  }

  function hide() {
    if (leaveTimer !== null) clearTimeout(leaveTimer);
    leaveTimer = null;
    isOver = false;
  }

  function handleDragOver(event: DragEvent) {
    if (!isExternalFileDrag(event)) return;
    // ドロップを受け取る（受け取らないと、WebViewがファイルを開く）
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
    isOver = true;
    // ウィンドウの上にいる間は`dragover`が届き続ける。届かなくなったら、外へ出たとみなす
    if (leaveTimer !== null) clearTimeout(leaveTimer);
    leaveTimer = setTimeout(hide, LEAVE_DELAY_MS);
  }

  function handleDrop(event: DragEvent) {
    if (!isExternalFileDrag(event)) return;
    event.preventDefault();
    hide();
    const names = event.dataTransfer ? droppedNames(event.dataTransfer) : [];
    if (names.length > 0) void importer.importDropped(names);
  }
</script>

<svelte:window
  ondragstart={() => (isInternalDrag = true)}
  ondragend={() => (isInternalDrag = false)}
  ondragover={handleDragOver}
  ondrop={handleDrop}
/>

{#if isOver}
  <div class="drop-overlay" aria-hidden="true">
    <div class="drop-message">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        class="w-10 h-10"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        stroke-width="1.5"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M12 16V4m0 12l-4-4m4 4l4-4M4 17v2a1 1 0 001 1h14a1 1 0 001-1v-2"
        />
      </svg>
      <p class="m-0">{m.importDialog.dropHere}</p>
    </div>
  </div>
{/if}

<style>
  @reference "../../app.css";

  /* ウィンドウ全体に重ねる（下の画面への操作は通さないが、ドロップは`window`で受け取る） */
  .drop-overlay {
    @apply fixed inset-0 z-50 flex items-center justify-center p-6 pointer-events-none;
    background-color: color-mix(in srgb, var(--color-base-100) 80%, transparent);
  }

  .drop-message {
    @apply flex flex-col items-center gap-3 px-10 py-8 border-2 border-dashed border-primary rounded-xl text-primary text-lg font-semibold bg-base-200;
  }
</style>
