<!--
  @component MetadataEditor
  曲の情報の編集画面（別のウィンドウ`/track-editor`の中身。ADR-094）。

  - 1曲の編集: 「タグ」「並び順」「歌詞」「アルバムアート」「ファイル情報」のタブ。開いた時にファイルから
    タグを読み（作曲者・コメント・歌詞などは、データベースに保存していないため）、保存するとすべての項目を
    ファイルへ書き込む（空にした項目は、タグから取り除く）。タグを変えていなければ、タグは書き込まない
  - 一括編集: 「タグ」「アルバムアート」のタブ。入力した項目だけを、選んだ曲のファイルへ書き込む
    （タイトルとその読み・トラック番号・歌詞は扱わない）
  - 「並び順」は、並び順に使う読み（ソート用のタグ）。空の項目は、表示用の名前で並べる
  - 「アルバムアート」で選んだ変更は、「保存」でタグの後に書き込む（`AlbumArtEditor`）
  - 保存して知らせることがなければ、画面を閉じる。一部の曲に書き込めなかった時などは、閉じずに知らせる

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
  import { useRemoveAlbumArtMutation, useSetAlbumArtMutation } from '#lib/queries/albumArt.js';
  import { formatDateTime, formatDuration, formatFileSize } from '#lib/utils/format.js';
  import { albumArtUrl } from '#lib/utils/albumArt.js';
  import { toErrorMessage } from '#lib/stores/error.svelte.js';
  import { TabPanel, Tabs } from '#lib/components/ui/index.js';
  import AlbumArt from './AlbumArt.svelte';
  import AlbumArtEditor, { type AlbumArtChange } from './AlbumArtEditor.svelte';
  import { m } from '#lib/i18n/i18n.svelte.js';

  interface Props {
    tracks: Track[];
    /** 画面を閉じる（キャンセル・保存の後） */
    onClose: () => void;
  }

  let { tracks, onClose }: Props = $props();

  const updateMetadataMutation = useUpdateTrackMetadataMutation();
  const updateMultipleTracksMutation = useUpdateMultipleTracksMutation();
  const setAlbumArtMutation = useSetAlbumArtMutation();
  const removeAlbumArtMutation = useRemoveAlbumArtMutation();
  const showInFolderMutation = useShowInFolderMutation();

  // 編集する曲は、開いた時点のものを使う（開いている間に一覧が更新されても、入力を保つ）
  // svelte-ignore state_referenced_locally
  const isSingleEdit = tracks.length === 1;
  // svelte-ignore state_referenced_locally
  const track = tracks[0];
  // svelte-ignore state_referenced_locally
  const trackCount = tracks.length;
  // アルバムアートを書き込む曲（ファイルが見つからない曲は除く）
  // svelte-ignore state_referenced_locally
  const artTargetIds = tracks.filter((t) => !t.isMissing).map((t) => t.id);

  type Tab = 'tags' | 'sort' | 'lyrics' | 'albumArt' | 'file';
  const tabs: Tab[] = isSingleEdit
    ? ['tags', 'sort', 'lyrics', 'albumArt', 'file']
    : ['tags', 'albumArt'];
  const tabItems = $derived(tabs.map((tab) => ({ value: tab, label: m.metadataEditor.tabs[tab] })));
  let activeTab = $state<Tab>('tags');

  // 保存する時に行う、アルバムアートの変更（「アルバムアート」のタブで選ぶ）
  let artChange = $state<AlbumArtChange>({ kind: 'none' });

  // 1曲の編集では、ファイルからタグを読む（読めるまで・読めない場合は、保存できない）
  const tagsQuery = isSingleEdit ? useTrackTagsQuery(track.id) : null;
  const tagsError = $derived(tagsQuery?.error ? toErrorMessage(tagsQuery.error) : null);

  let form = $state<MetadataForm>(emptyForm());
  // 読んだタグを入力欄へ入れたか（入れた後は、読み直しがあっても入力を上書きしない）
  let formLoaded = $state(!isSingleEdit);
  // 1曲の編集で、読んだタグの値（保存する時に、タグを変えたかを比べる）
  let loadedMetadata: string | null = null;

  $effect(() => {
    const tags = tagsQuery?.data;
    if (tags && !formLoaded) {
      form = formFromTags(tags);
      loadedMetadata = JSON.stringify(metadataFromForm(form, 'single'));
      formLoaded = true;
    }
  });

  let error = $state<string | null>(null);
  let validationError = $state<string | null>(null);
  let isSaving = $state(false);
  // 保存した後に知らせること（あれば、画面を閉じずに出し、「閉じる」だけを押せるようにする）
  let notices = $state<string[] | null>(null);
  const canEdit = $derived(formLoaded && !isSaving && notices === null);

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
  const unchangedPlaceholder = $derived(isSingleEdit ? undefined : m.metadataEditor.unchanged);

  /**
   * タグを書き込む（変えた項目がなければ書き込まない）
   *
   * @returns 知らせること。1曲も書き込めなかった時は、例外を投げる（何も変えていない）
   */
  async function saveTags(): Promise<string[]> {
    const notes: string[] = [];
    if (isSingleEdit) {
      const metadata = metadataFromForm(form, 'single');
      if (JSON.stringify(metadata) === loadedMetadata) return notes;
      // ファイルのタグへ書き込む（空にした項目は、タグからも取り除く）
      const organized = await updateMetadataMutation.mutateAsync({ trackId: track.id, metadata });
      if (organized && organized.notMovedCount > 0) {
        notes.push(m.musicFolder.keepOrganizedNotMoved(organized.notMovedCount));
      }
      return notes;
    }

    // 入力した項目だけを、各トラックのファイルのタグへ書き込む
    const metadata = metadataFromForm(form, 'bulk');
    if (Object.keys(metadata).length === 0) return notes;
    const result = await updateMultipleTracksMutation.mutateAsync({
      trackIds: tracks.map((t) => t.id),
      metadata
    });
    if (result.failedCount > 0) {
      // 1曲も更新できなかった場合は、アルバムアートも書き込まずに理由を出す
      if (result.updatedCount === 0) {
        throw new Error(m.metadataEditor.bulkAllFailed(result.errors[0] ?? ''));
      }
      notes.push(m.metadataEditor.bulkPartiallyFailed(result.updatedCount, result.failedCount));
    }
    if (result.organized && result.organized.notMovedCount > 0) {
      notes.push(m.musicFolder.keepOrganizedNotMoved(result.organized.notMovedCount));
    }
    return notes;
  }

  /**
   * アルバムアートを書き込む（「アルバムアート」のタブで選んだ変更）
   *
   * @returns 知らせること（タグは書き込んだ後のため、失敗も知らせることにする）
   */
  async function saveAlbumArt(): Promise<string[]> {
    const change = artChange;
    if (change.kind === 'none' || artTargetIds.length === 0) return [];
    try {
      const result =
        change.kind === 'set'
          ? await setAlbumArtMutation.mutateAsync({
              trackIds: artTargetIds,
              token: change.picked.token
            })
          : await removeAlbumArtMutation.mutateAsync(artTargetIds);
      artChange = { kind: 'none' };
      if (result.failedCount === 0) return [];
      return [
        result.updatedCount === 0
          ? m.albumArtEditor.allFailed(result.errors[0] ?? '')
          : m.albumArtEditor.partiallyFailed(result.failedCount)
      ];
    } catch (e) {
      return [m.metadataEditor.tagsSavedArtFailed(toErrorMessage(e))];
    }
  }

  async function handleSave() {
    error = null;
    const errors = validateForm(form);
    if (errors.length > 0) {
      validationError = errors.join(', ');
      activeTab = 'tags';
      return;
    }
    validationError = null;
    isSaving = true;

    try {
      // タグを先に書き込む（書き込めなかった場合は、アルバムアートも書き込まない）
      const notes = await saveTags();
      notes.push(...(await saveAlbumArt()));
      // キャッシュの無効化はミューテーションのonSuccessで、メインウィンドウへの反映は
      // Rustが送る`TracksEdited`で行う
      if (notes.length > 0) {
        notices = notes;
      } else {
        onClose();
      }
    } catch (e) {
      error = toErrorMessage(e);
    } finally {
      isSaving = false;
    }
  }

  // Escで閉じる（保存の間は閉じない）
  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && !event.defaultPrevented && !isSaving) {
      event.preventDefault();
      onClose();
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
          aria-label={m.fields[number]}
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
        aria-label={m.fields[total]}
        placeholder={unchangedPlaceholder}
        disabled={!canEdit}
      />
    </div>
  </div>
{/snippet}

<!-- 候補付きの文字列の入力欄 -->
{#snippet textField(field: TextField, hint: string, suggestions: string[] | null)}
  <div class="form-group">
    <label for="metadata-{field}" class="form-label">{m.fields[field]}</label>
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

<!-- 並び順に使う読みの入力欄（空の時は、並びに使われる表示用の名前を薄く出す） -->
{#snippet sortFields()}
  {#if isSingleEdit}
    {@render textField('titleSort', form.title, null)}
  {/if}
  {@render textField('artistSort', placeholder(form.artist), null)}
  {@render textField('albumArtistSort', placeholder(form.albumArtist), null)}
  {@render textField('albumSort', placeholder(form.album), null)}
{/snippet}

<svelte:window onkeydown={handleKeydown} />

<div class="editor">
  <!-- 編集する曲（1曲は曲名とアーティスト、一括編集は曲数） -->
  <header class="editor-header">
    {#if isSingleEdit}
      <div class="header-art">
        <AlbumArt src={albumArtUrl(track.id)} alt="" rounded="md" placeholderType="music" />
      </div>
      <div class="min-w-0">
        <p class="header-title">{track.title || track.fileName}</p>
        <p class="header-sub">
          {[track.artist, track.album].filter((value) => value).join(' — ')}
        </p>
      </div>
    {:else}
      <p class="header-title">{m.metadataEditor.bulkEditTitle(trackCount)}</p>
    {/if}
  </header>

  <div class="editor-body">
    <Tabs bind:value={activeTab} tabs={tabItems} disabled={isSaving}>
      {#if !isSingleEdit && activeTab === 'tags'}
        <div class="message-info mb-4">{m.metadataEditor.bulkHint}</div>
      {/if}

      {#if tagsError}
        <div class="message-error mb-4">{m.metadataEditor.tagsUnavailable(tagsError)}</div>
      {:else if !formLoaded}
        <p class="text-sm text-text-muted mt-0 mb-4" role="status">
          {m.metadataEditor.loadingTags}
        </p>
      {/if}

      <form onsubmit={(e) => e.preventDefault()}>
        <!-- タグ -->
        <TabPanel value="tags">
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
          {@render textField(
            'album',
            placeholder(m.metadataEditor.albumPlaceholder),
            albumSuggestions
          )}
          <div class="field-row">
            {@render textField('composer', placeholder(''), composerSuggestions)}
            {@render textField(
              'genre',
              placeholder(m.metadataEditor.genrePlaceholder),
              genreSuggestions
            )}
          </div>
          <p class="mt-0 mb-3 text-xs text-text-muted">{m.metadataEditor.multiValueHint}</p>
          <div class="field-row four">
            <div class="form-group">
              <label for="metadata-year" class="form-label">{m.fields.year}</label>
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
              <label for="metadata-bpm" class="form-label">{m.fields.bpm}</label>
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
            <label for="metadata-comment" class="form-label">{m.fields.comment}</label>
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
              <span class="text-xs text-text-muted">{m.metadataEditor.compilationHint}</span>
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

          <!-- 並び順（一括編集はタグの下） -->
          {#if !isSingleEdit}
            <fieldset class="sort-section">
              <legend>{m.metadataEditor.sortHeading}</legend>
              <p class="text-xs text-text-muted mt-0 mb-3">{m.metadataEditor.sortHint}</p>
              {@render sortFields()}
            </fieldset>
          {/if}
        </TabPanel>

        <!-- 並び順（1曲の編集はタブ） -->
        {#if isSingleEdit}
          <TabPanel value="sort">
            <p class="text-sm text-text-muted mt-0 mb-4">{m.metadataEditor.sortHint}</p>
            {@render sortFields()}
          </TabPanel>
        {/if}

        <!-- 歌詞（1曲の編集だけ） -->
        {#if isSingleEdit}
          <TabPanel value="lyrics">
            <textarea
              class="form-input lyrics-input"
              rows="14"
              bind:value={form.lyrics}
              aria-label={m.fields.lyrics}
              placeholder={m.metadataEditor.lyricsPlaceholder}
              disabled={!canEdit}></textarea>
          </TabPanel>
        {/if}

        <!-- アルバムアート（選んだ変更は、保存でタグの後に書き込む） -->
        <TabPanel value="albumArt">
          <AlbumArtEditor
            {tracks}
            bind:change={artChange}
            disabled={isSaving || notices !== null}
          />
        </TabPanel>

        {#if isSingleEdit}
          <!-- ファイル情報（読み取り専用） -->
          <TabPanel value="file" focusable>
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
              <dd>{track.format}</dd>
              <dt>{m.metadataEditor.file.bitrate}</dt>
              <dd>{track.bitrate ? m.metadataEditor.file.kbps(track.bitrate) : '--'}</dd>
              <dt>{m.metadataEditor.file.sampleRate}</dt>
              <dd>{track.sampleRate ? m.metadataEditor.file.hz(track.sampleRate) : '--'}</dd>
              <dt>{m.metadataEditor.file.bitDepth}</dt>
              <dd>
                {track.details.bitDepth ? m.metadataEditor.file.bits(track.details.bitDepth) : '--'}
              </dd>
              <dt>{m.metadataEditor.file.channels}</dt>
              <dd>{track.details.channels ?? '--'}</dd>
              <dt>{m.metadataEditor.file.duration}</dt>
              <dd>{formatDuration(track.duration)}</dd>
              <dt>{m.metadataEditor.file.size}</dt>
              <dd>{formatFileSize(track.fileSize)}</dd>
              <dt>{m.metadataEditor.file.addedAt}</dt>
              <dd>{formatDateTime(track.createdAt)}</dd>
              <dt>{m.metadataEditor.file.lastPlayedAt}</dt>
              <dd>
                {track.lastPlayedAt
                  ? formatDateTime(track.lastPlayedAt)
                  : m.metadataEditor.file.never}
              </dd>
              <dt>{m.metadataEditor.file.playCount}</dt>
              <dd>{m.metadataEditor.file.times(track.playCount)}</dd>
              <dt>{m.metadataEditor.file.skipCount}</dt>
              <dd>{m.metadataEditor.file.times(track.skipCount)}</dd>
            </dl>
          </TabPanel>
        {/if}

        {#if activeTab !== 'file' && activeTab !== 'albumArt'}
          <p class="text-xs text-text-muted mt-3 mb-0">{m.metadataEditor.writesToFile}</p>
        {/if}

        {#if validationError}
          <div class="message-error mt-4">{validationError}</div>
        {/if}

        {#if error}
          <div class="message-error mt-4">{error}</div>
        {/if}
      </form>
    </Tabs>
  </div>

  <!-- 保存した後に知らせること（閉じずに出す） -->
  {#if notices}
    <div class="editor-notices" role="status">
      <p class="m-0 font-semibold">{m.metadataEditor.savedWithWarnings}</p>
      <ul>
        {#each notices as notice, index (index)}
          <li>{notice}</li>
        {/each}
      </ul>
    </div>
  {/if}

  <footer class="editor-footer">
    {#if notices}
      <button class="btn-primary" onclick={onClose}>{m.common.close}</button>
    {:else}
      <button class="btn-secondary" onclick={onClose} disabled={isSaving}>{m.common.cancel}</button>
      <button class="btn-primary" onclick={handleSave} disabled={!canEdit}>
        {isSaving ? m.common.saving : m.common.save}
      </button>
    {/if}
  </footer>
</div>

<style>
  @reference "../../app.css";

  /* ウィンドウいっぱいに出し、項目だけをスクロールする（見出しとボタンは常に見える） */
  .editor {
    @apply h-screen flex flex-col bg-base-100 text-text-primary;
  }

  .editor-header {
    @apply flex items-center gap-3 px-6 py-3 border-b border-border shrink-0;
  }

  .header-art {
    @apply w-12 h-12 shrink-0 rounded-md overflow-hidden;
  }

  .header-title {
    @apply m-0 text-sm font-semibold text-text-primary truncate;
  }

  .header-sub {
    @apply m-0 mt-0.5 text-xs text-text-muted truncate;
  }

  .editor-body {
    @apply flex-1 min-h-0 overflow-y-auto px-6 pt-3 pb-4;
  }

  .editor-notices {
    @apply mx-6 mb-3 p-3 rounded-md bg-warning/20 text-warning text-sm shrink-0;
  }

  .editor-notices ul {
    @apply m-0 mt-1 pl-5;
  }

  .editor-footer {
    @apply flex justify-end gap-2 px-6 py-3 border-t border-border shrink-0;
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

  /* 一括編集の、並び順の項目のまとまり */
  .sort-section {
    @apply mt-4 mb-0 mx-0 px-4 pt-2 pb-0 border border-border rounded;
  }

  .sort-section legend {
    @apply px-2 text-sm font-semibold text-text-secondary;
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
