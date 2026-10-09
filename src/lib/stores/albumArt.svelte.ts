/**
 * アルバムアートの表示の更新と、アルバムアートの画面の状態
 *
 * アルバムアートのURLはトラックIDだけで決まるため、ファイルの画像を書き換えても変わらない。
 * 書き換えた曲には版を持たせてURLに付け、表示中の`<img>`に読み直させる（`#lib/utils/albumArt`）。
 */
import { SvelteMap } from 'svelte/reactivity';
import type { Track } from '#lib/types/models.js';

/** 曲ごとの、アルバムアートを書き換えた回数（書き換えていない曲は持たない） */
const versions = new SvelteMap<string, number>();

/** 曲のアルバムアートの版（書き換えていなければ0。読み取りは変更に追随する） */
export function albumArtVersion(trackId: string): number {
  return versions.get(trackId) ?? 0;
}

/** 曲のアルバムアートを読み直させる（アルバムアートを書き換えた後に呼ぶ） */
export function refreshAlbumArt(trackIds: Iterable<string>): void {
  for (const trackId of trackIds) {
    versions.set(trackId, (versions.get(trackId) ?? 0) + 1);
  }
}

class AlbumArtDialogState {
  /** アルバムアートの画面で扱う曲（閉じている間はnull） */
  tracks = $state.raw<Track[] | null>(null);

  /** 曲のアルバムアートの画面を開く（曲がなければ開かない） */
  open(tracks: Track[]): void {
    if (tracks.length > 0) this.tracks = tracks;
  }

  close(): void {
    this.tracks = null;
  }
}

/** アルバムアートの画面（メニューなどから開き、`(app)/+layout.svelte`が表示する） */
export const albumArtDialog = new AlbumArtDialogState();
