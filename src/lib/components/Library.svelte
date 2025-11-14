<script lang="ts">
  import { useTracksQuery } from '$lib/queries/tracks';
  import type { Track } from '$lib/types/models';

  type ViewMode = 'grid' | 'list';

  let viewMode = $state<ViewMode>('grid');

  const tracksQuery = useTracksQuery();

  // クエリ結果を取得
  const isLoading = $derived(tracksQuery.isLoading);
  const isError = $derived(tracksQuery.isError);
  const error = $derived(tracksQuery.error);
  const tracks = $derived(tracksQuery.data);

  /**
   * トラック情報を表示用にフォーマット
   */
  function formatTrackInfo(track: Track): string {
    const title = track.title || track.fileName;
    const artist = track.artist || '不明なアーティスト';
    const album = track.album || '不明なアルバム';
    return `${title} - ${artist} - ${album}`;
  }

  /**
   * 再生時間をフォーマット (秒 -> mm:ss)
   */
  function formatDuration(seconds: number | null): string {
    if (!seconds) return '--:--';
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  }

  /**
   * 表示モードを切り替え
   */
  function toggleViewMode() {
    viewMode = viewMode === 'grid' ? 'list' : 'grid';
  }
</script>

<div class="library-container">
  <!-- ヘッダー -->
  <div class="library-header">
    <h2>音楽ライブラリ</h2>
    <button onclick={toggleViewMode} class="view-toggle">
      {viewMode === 'grid' ? 'リスト表示' : 'グリッド表示'}
    </button>
  </div>

  <!-- コンテンツ -->
  <div class="library-content">
    {#if isLoading}
      <div class="loading">読み込み中...</div>
    {:else if isError}
      <div class="error">
        エラーが発生しました: {error?.message || '不明なエラー'}
      </div>
    {:else if tracks && tracks.length > 0}
      <div class="tracks-{viewMode}">
        {#each tracks as track (track.id)}
          <div class="track-item">
            {#if viewMode === 'grid'}
              <!-- グリッド表示 -->
              <div class="track-card">
                <div class="track-icon">🎵</div>
                <div class="track-info">
                  <div class="track-title">{track.title || track.fileName}</div>
                  <div class="track-artist">{track.artist || '不明なアーティスト'}</div>
                  <div class="track-album">{track.album || '不明なアルバム'}</div>
                </div>
              </div>
            {:else}
              <!-- リスト表示 -->
              <div class="track-row">
                <div class="track-col track-col-title">
                  {track.title || track.fileName}
                </div>
                <div class="track-col track-col-artist">
                  {track.artist || '不明なアーティスト'}
                </div>
                <div class="track-col track-col-album">
                  {track.album || '不明なアルバム'}
                </div>
                <div class="track-col track-col-duration">
                  {formatDuration(track.duration)}
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="empty">
        <p>音楽ライブラリが空です</p>
        <p>フォルダをインポートして音楽を追加してください</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .library-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 1rem;
  }

  .library-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
    padding-bottom: 1rem;
    border-bottom: 1px solid #e0e0e0;
  }

  .library-header h2 {
    margin: 0;
    font-size: 1.5rem;
    font-weight: 600;
  }

  .view-toggle {
    padding: 0.5rem 1rem;
    background-color: #007bff;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.9rem;
  }

  .view-toggle:hover {
    background-color: #0056b3;
  }

  .library-content {
    flex: 1;
    overflow-y: auto;
  }

  .loading,
  .error,
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #666;
  }

  .error {
    color: #dc3545;
  }

  .empty p {
    margin: 0.5rem 0;
  }

  /* グリッド表示 */
  .tracks-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 1rem;
  }

  .track-card {
    padding: 1rem;
    background-color: #f8f9fa;
    border-radius: 8px;
    cursor: pointer;
    transition:
      transform 0.2s,
      box-shadow 0.2s;
  }

  .track-card:hover {
    transform: translateY(-2px);
    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.1);
  }

  .track-icon {
    font-size: 3rem;
    text-align: center;
    margin-bottom: 0.5rem;
  }

  .track-info {
    text-align: center;
  }

  .track-title {
    font-weight: 600;
    margin-bottom: 0.25rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .track-artist,
  .track-album {
    font-size: 0.85rem;
    color: #666;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* リスト表示 */
  .tracks-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .track-row {
    display: grid;
    grid-template-columns: 2fr 1.5fr 1.5fr 100px;
    gap: 1rem;
    padding: 0.75rem 1rem;
    background-color: #f8f9fa;
    border-radius: 4px;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .track-row:hover {
    background-color: #e9ecef;
  }

  .track-col {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .track-col-title {
    font-weight: 600;
  }

  .track-col-artist,
  .track-col-album {
    color: #666;
  }

  .track-col-duration {
    text-align: right;
    color: #666;
  }

  @media (prefers-color-scheme: dark) {
    .library-header {
      border-bottom-color: #444;
    }

    .track-card,
    .track-row {
      background-color: #2a2a2a;
    }

    .track-card:hover {
      box-shadow: 0 4px 8px rgba(255, 255, 255, 0.1);
    }

    .track-row:hover {
      background-color: #333;
    }

    .track-artist,
    .track-album,
    .track-col-artist,
    .track-col-album,
    .track-col-duration {
      color: #aaa;
    }
  }
</style>
