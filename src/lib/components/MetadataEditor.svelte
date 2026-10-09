<!--
  @component MetadataEditor
  曲のタグの編集画面。

  - 1曲の編集: 「タグ」「歌詞」「ファイル情報」のタブ。開いた時にファイルからタグを読み
    （作曲者・コメント・歌詞などは、データベースに保存していないため）、保存するとすべての項目を
    ファイルへ書き込む（空にした項目は、タグから取り除く）
  - 一括編集: 入力した項目だけを、選んだ曲のファイルへ書き込む（タイトル・トラック番号・歌詞は扱わない）

  入力欄の値と`Metadata`の変換・検証は`#lib/utils/metadataForm`が行う。
-->
<script lang="ts">
  import type { Track } from '#lib/types/models.js';
  import {
    useShowInFolderMutation,
    useTrackTagsQuery,
    useUniqueAlbumsQuery,
    useUniqueArtistsQuery,
    useUniqueGenresQuery,
    useUpdateMultipleTracksMutation,
    useUpdateTrackMetadataMutation
  } from '#lib/queries/tracks.js';
  import {
    emptyForm,
    formFromTags,
    metadataFromForm,
    suggestionsFor,
    validateForm,
    type MetadataForm,
    type NumberField,
    type TextField
  } from '#lib/utils/metadataForm.js';
  import { formatDateTime, formatDuration, formatFileSize } from '#lib/utils/format.js';
  import { showWarning, toErrorMessage } from '#lib/stores/error.svelte.js';
  import { Modal } from '#lib/components/ui/index.js';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    tracks: Track[];
    onClose: () => void;
    onSave?: () => void;
  }

  let { tracks, onClose, onSave }: Props = $props();

  const updateMetadataMutation = useUpdateTrackMetadataMutation();
  const updateMultipleTracksMutation = useUpdateMultipleTracksMutation();
  const showInFolderMutation = useShowInFolderMutation();

  // 編集する曲は、開いた時点のものを使う（開いている間に一覧が更新されても、入力を保つ）
  // svelte-ignore state_referenced_locally
  const isSingleEdit = tracks.length === 1;
  // svelte-ignore state_referenced_locally
  const track = tracks[0];
  // svelte-ignore state_referenced_locally
  const dialogTitle = isSingleEdit
    ? m.metadataEditor.editTitle
    : m.metadataEditor.bulkEditTitle(tracks.length);

  type Tab = 'tags' | 'lyrics' | 'file';
  const tabs: Tab[] = ['tags', 'lyrics', 'file'];
  let activeTab = $state<Tab>('tags');

  // 1曲の編集では、ファイルからタグを読む（読めるまで・読めない場合は、保存できない）
  const tagsQuery = isSingleEdit ? useTrackTagsQuery(track.id) : null;
  const tagsError = $derived(tagsQuery?.error ? toErrorMessage(tagsQuery.error) : null);

  let form = $state<MetadataForm>(emptyForm());
  // 読んだタグを入力欄へ入れたか（入れた後は、読み直しがあっても入力を上書きしない）
  let formLoaded = $state(!isSingleEdit);

  $effect(() => {
    const tags = tagsQuery?.data;
    if (tags && !formLoaded) {
      form = formFromTags(tags);
      formLoaded = true;
    }
  });

  let error = $state<string | null>(null);
  let validationError = $state<string | null>(null);
  let isSaving = $state(false);
  const canEdit = $derived(formLoaded && !isSaving);

  const labels: Record<TextField | NumberField, string> = {
    title: m.fields.title,
    artist: m.fields.artist,
    album: m.fields.album,
    albumArtist: m.fields.albumArtist,
    composer: m.fields.composer,
    genre: m.fields.genre,
    grouping: m.fields.grouping,
    comment: m.fields.comment,
    lyrics: m.fields.lyrics,
    year: m.fields.year,
    trackNumber: m.fields.trackNumber,
    trackTotal: m.fields.trackTotal,
    discNumber: m.fields.discNumber,
    discTotal: m.fields.discTotal,
    bpm: m.fields.bpm
  };

  // 入力の候補（ライブラリにある値）
  const artistsQuery = useUniqueArtistsQuery();
  const albumsQuery = useUniqueAlbumsQuery();
  const genresQuery = useUniqueGenresQuery();
  const artistSuggestions = $derived(suggestionsFor(artistsQuery.data, form.artist));
  const albumArtistSuggestions = $derived(suggestionsFor(artistsQuery.data, form.albumArtist));
  const composerSuggestions = $derived(suggestionsFor(artistsQuery.data, form.composer));
  const albumSuggestions = $derived(suggestionsFor(albumsQuery.data, form.album));
  const genreSuggestions = $derived(suggestionsFor(genresQuery.data, form.genre));

  const placeholder = (text: string) => (isSingleEdit ? text : m.metadataEditor.unchanged);
  const unchangedPlaceholder = isSingleEdit ? undefined : m.metadataEditor.unchanged;

  async function handleSave() {
    error = null;
    const errors = validateForm(form, labels);
    if (errors.length > 0) {
      validationError = errors.join(', ');
      return;
    }
    validationError = null;
    isSaving = true;

    try {
      if (isSingleEdit) {
        // ファイルのタグへ書き込む（空にした項目は、タグからも取り除く）
        await updateMetadataMutation.mutateAsync({
          trackId: track.id,
          metadata: metadataFromForm(form, 'single')
        });
      } else {
        // 入力した項目だけを、各トラックのファイルのタグへ書き込む
        const result = await updateMultipleTracksMutation.mutateAsync({
          trackIds: tracks.map((t) => t.id),
          metadata: metadataFromForm(form, 'bulk')
        });

        if (result.failedCount > 0) {
          if (result.updatedCount === 0) {
            // 1曲も更新できなかった場合は、画面を閉じずに理由を表示する
            error = m.metadataEditor.bulkAllFailed(result.errors[0] ?? '');
            return;
          }
          showWarning(
            m.metadataEditor.bulkPartiallyFailed(result.updatedCount, result.failedCount)
          );
        }
      }

      // キャッシュ無効化はミューテーションのonSuccessで自動実行
      onSave?.();
      onClose();
    } catch (e) {
      error = toErrorMessage(e);
    } finally {
      isSaving = false;
    }
  }
</script>

<!-- 番号と総数の入力欄（トラック・ディスク。一括編集では、番号を1曲ごとの項目として出さない） -->
{#snippet numberPair(
  label: string,
  number: NumberField | null,
  total: NumberField,
  idPrefix: string
)}
  <div class="form-group">
    <span class="form-label" id="{idPrefix}-label">{label}</span>
    <div class="flex items-center gap-2" role="group" aria-labelledby="{idPrefix}-label">
      {#if number}
        <input
          type="text"
          inputmode="numeric"
          class="form-input number-input"
          bind:value={form[number]}
          aria-label={labels[number]}
          placeholder={unchangedPlaceholder}
          disabled={!canEdit}
        />
        <span class="text-text-muted">{m.metadataEditor.of}</span>
      {/if}
      <input
        type="text"
        inputmode="numeric"
        class="form-input number-input"
        bind:value={form[total]}
        aria-label={labels[total]}
        placeholder={unchangedPlaceholder}
        disabled={!canEdit}
      />
    </div>
  </div>
{/snippet}

<!-- 候補付きの文字列の入力欄 -->
{#snippet textField(field: TextField, hint: string, suggestions: string[] | null)}
  <div class="form-group">
    <label for="metadata-{field}" class="form-label">{labels[field]}</label>
    <input
      id="metadata-{field}"
      type="text"
      class="form-input"
      bind:value={form[field]}
      placeholder={hint}
      list={suggestions ? `metadata-${field}-suggestions` : undefined}
      autocomplete="off"
      disabled={!canEdit}
    />
    {#if suggestions}
      <datalist id="metadata-{field}-suggestions">
        {#each suggestions as suggestion (suggestion)}
          <option value={suggestion}></option>
        {/each}
      </datalist>
    {/if}
  </div>
{/snippet}

<Modal open {onClose} title={dialogTitle} dismissible={!isSaving} class="max-w-2xl">
  {#if isSingleEdit}
    <div class="tab-list" role="tablist">
      {#each tabs as tab (tab)}
        <button
          type="button"
          class="tab"
          class:active={activeTab === tab}
          role="tab"
          aria-selected={activeTab === tab}
          onclick={() => (activeTab = tab)}
        >
          {m.metadataEditor.tabs[tab]}
        </button>
      {/each}
    </div>
  {:else}
    <div class="message-info mb-4">{m.metadataEditor.bulkHint}</div>
  {/if}

  {#if tagsError}
    <div class="message-error mb-4">{m.metadataEditor.tagsUnavailable(tagsError)}</div>
  {:else if !formLoaded}
    <p class="text-sm text-text-muted mt-0 mb-4" role="status">{m.metadataEditor.loadingTags}</p>
  {/if}

  <form onsubmit={(e) => e.preventDefault()}>
    <!-- タグ -->
    <div hidden={activeTab !== 'tags'}>
      {#if isSingleEdit}
        {@render textField('title', m.metadataEditor.titlePlaceholder, null)}
      {/if}
      <div class="field-row">
        {@render textField(
          'artist',
          placeholder(m.metadataEditor.artistPlaceholder),
          artistSuggestions
        )}
        {@render textField('albumArtist', placeholder(''), albumArtistSuggestions)}
      </div>
      {@render textField('album', placeholder(m.metadataEditor.albumPlaceholder), albumSuggestions)}
      <div class="field-row">
        {@render textField('composer', placeholder(''), composerSuggestions)}
        {@render textField(
          'genre',
          placeholder(m.metadataEditor.genrePlaceholder),
          genreSuggestions
        )}
      </div>
      <div class="field-row four">
        <div class="form-group">
          <label for="metadata-year" class="form-label">{labels.year}</label>
          <input
            id="metadata-year"
            type="text"
            inputmode="numeric"
            class="form-input"
            bind:value={form.year}
            placeholder={placeholder(m.metadataEditor.yearPlaceholder)}
            disabled={!canEdit}
          />
        </div>
        {@render numberPair(
          m.metadataEditor.track,
          isSingleEdit ? 'trackNumber' : null,
          'trackTotal',
          'metadata-track'
        )}
        {@render numberPair(m.metadataEditor.disc, 'discNumber', 'discTotal', 'metadata-disc')}
        <div class="form-group">
          <label for="metadata-bpm" class="form-label">{labels.bpm}</label>
          <input
            id="metadata-bpm"
            type="text"
            inputmode="numeric"
            class="form-input"
            bind:value={form.bpm}
            placeholder={unchangedPlaceholder}
            disabled={!canEdit}
          />
        </div>
      </div>
      {@render textField('grouping', placeholder(''), null)}
      <div class="form-group">
        <label for="metadata-comment" class="form-label">{labels.comment}</label>
        <textarea
          id="metadata-comment"
          class="form-input"
          rows="2"
          bind:value={form.comment}
          placeholder={unchangedPlaceholder}
          disabled={!canEdit}></textarea>
      </div>

      <!-- コンピレーション: 1曲の編集は印の有無、一括編集は「変更しない」も選べる -->
      {#if isSingleEdit}
        <label class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer">
          <input
            type="checkbox"
            class="w-4 h-4"
            checked={form.compilation === 'on'}
            onchange={(e) => (form.compilation = e.currentTarget.checked ? 'on' : 'off')}
            disabled={!canEdit}
          />
          {m.fields.compilation}
          <span class="text-xs text-text-muted">（{m.metadataEditor.compilationHint}）</span>
        </label>
      {:else}
        <div class="form-group">
          <label for="metadata-compilation" class="form-label">{m.fields.compilation}</label>
          <select
            id="metadata-compilation"
            class="form-input"
            bind:value={form.compilation}
            disabled={!canEdit}
          >
            <option value="unchanged">{m.metadataEditor.unchanged}</option>
            <option value="on">{m.metadataEditor.compilationOn}</option>
            <option value="off">{m.metadataEditor.compilationOff}</option>
          </select>
        </div>
      {/if}
    </div>

    <!-- 歌詞（1曲の編集だけ） -->
    {#if isSingleEdit}
      <div hidden={activeTab !== 'lyrics'}>
        <textarea
          class="form-input lyrics-input"
          rows="14"
          bind:value={form.lyrics}
          aria-label={labels.lyrics}
          placeholder={m.metadataEditor.lyricsPlaceholder}
          disabled={!canEdit}></textarea>
      </div>

      <!-- ファイル情報（読み取り専用） -->
      <div hidden={activeTab !== 'file'}>
        <dl class="file-info">
          <dt>{m.metadataEditor.file.fileName}</dt>
          <dd>{track.fileName}</dd>
          <dt>{m.metadataEditor.file.location}</dt>
          <dd>
            <span class="break-all select-text">{track.filePath}</span>
            {#if track.isMissing}
              <span class="block text-warning">{m.metadataEditor.file.missing}</span>
            {:else}
              <button
                type="button"
                class="btn-secondary text-xs mt-1"
                onclick={() => showInFolderMutation.mutate(track.id)}
              >
                {m.contextMenu.showInFolder}
              </button>
            {/if}
          </dd>
          <dt>{m.metadataEditor.file.format}</dt>
          <dd>{track.format.toUpperCase()}</dd>
          <dt>{m.metadataEditor.file.bitrate}</dt>
          <dd>{track.bitrate ? m.metadataEditor.file.kbps(track.bitrate) : '--'}</dd>
          <dt>{m.metadataEditor.file.sampleRate}</dt>
          <dd>{track.sampleRate ? m.metadataEditor.file.hz(track.sampleRate) : '--'}</dd>
          <dt>{m.metadataEditor.file.duration}</dt>
          <dd>{formatDuration(track.duration)}</dd>
          <dt>{m.metadataEditor.file.size}</dt>
          <dd>{formatFileSize(track.fileSize)}</dd>
          <dt>{m.metadataEditor.file.addedAt}</dt>
          <dd>{formatDateTime(track.createdAt)}</dd>
          <dt>{m.metadataEditor.file.lastPlayedAt}</dt>
          <dd>
            {track.lastPlayedAt ? formatDateTime(track.lastPlayedAt) : m.metadataEditor.file.never}
          </dd>
          <dt>{m.metadataEditor.file.playCount}</dt>
          <dd>{m.metadataEditor.file.times(track.playCount)}</dd>
          <dt>{m.metadataEditor.file.skipCount}</dt>
          <dd>{m.metadataEditor.file.times(track.skipCount)}</dd>
        </dl>
      </div>
    {/if}

    {#if activeTab !== 'file'}
      <p class="text-xs text-text-muted mt-3 mb-0">{m.metadataEditor.writesToFile}</p>
    {/if}

    {#if validationError}
      <div class="message-error mt-4">{validationError}</div>
    {/if}

    {#if error}
      <div class="message-error mt-4">{error}</div>
    {/if}
  </form>

  {#snippet footer()}
    <button class="btn-secondary" onclick={onClose} disabled={isSaving}>{m.common.cancel}</button>
    <button class="btn-primary" onclick={handleSave} disabled={!canEdit}>
      {isSaving ? m.common.saving : m.common.save}
    </button>
  {/snippet}
</Modal>

<style>
  @reference "../../app.css";

  .tab-list {
    @apply flex gap-1 mb-4 border-b border-border;
  }

  .tab {
    @apply bg-transparent border-none px-4 py-2 text-sm text-text-muted cursor-pointer border-b-2 border-transparent transition-colors;
    margin-bottom: -1px;
  }

  .tab:hover {
    @apply text-text-primary;
  }

  .tab.active {
    @apply text-primary border-primary font-semibold;
  }

  /* 2つの項目を横に並べる（狭い画面では縦に並べる） */
  .field-row {
    @apply grid gap-x-4;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .field-row.four {
    grid-template-columns: minmax(0, 1fr) auto auto minmax(0, 1fr);
  }

  .number-input {
    @apply w-16 text-center;
  }

  .lyrics-input {
    @apply font-mono text-sm leading-relaxed;
    resize: vertical;
  }

  .file-info {
    @apply grid gap-x-4 gap-y-2 m-0 text-sm;
    grid-template-columns: max-content minmax(0, 1fr);
  }

  .file-info dt {
    @apply text-text-muted;
  }

  .file-info dd {
    @apply m-0 text-text-primary;
  }
</style>
