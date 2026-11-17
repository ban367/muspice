/**
 * 設定ウィンドウで選んで、「適用」を待っている音楽フォルダ（#408）
 *
 * 音楽フォルダの「既定のフォルダを使う」「フォルダを変更...」は、選ぶだけで変えない。設定のページが
 * 「適用」で`apply`を呼び、音楽フォルダの画面（`MusicFolderSettings`）が登録した処理で変える
 * （外す曲がある時の確認・進み具合の表示は、その画面が持つ）。ウィンドウを閉じると、選んだものは捨てる。
 */

/** 選んだ音楽フォルダ（`source`は`set_music_folder`に渡す選び方。`path`は画面に出す場所） */
export interface MusicFolderChoice {
  source: 'default' | 'picked';
  path: string;
}

export class MusicFolderDraft {
  /** 選んで、「適用」を待っている音楽フォルダ（なければnull） */
  choice = $state.raw<MusicFolderChoice | null>(null);

  #apply: (() => Promise<void>) | null = null;

  /**
   * 「適用」で音楽フォルダを変える処理を登録する（音楽フォルダの画面が作られた時）
   * @returns 登録を外す関数
   */
  register(apply: () => Promise<void>): () => void {
    this.#apply = apply;
    return () => {
      if (this.#apply === apply) this.#apply = null;
    };
  }

  /** 選んだ音楽フォルダに変える（選んでいなければ、何もしない） */
  async apply(): Promise<void> {
    if (this.choice !== null && this.#apply !== null) await this.#apply();
  }
}
