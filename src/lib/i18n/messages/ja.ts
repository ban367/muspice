/**
 * 日本語のメッセージ（メッセージの定義の正）
 *
 * 英語（`en.ts`）は同じ形（`Messages`型）にする。引数のある文言は関数にする。
 * 画面からは`#lib/i18n`の`m`を通して使う（例: `m.common.cancel`、`m.common.trackCount(3)`）。
 */
import type { AppError } from '#lib/types/models.js';

export const ja = {
  common: {
    cancel: 'キャンセル',
    close: '閉じる',
    delete: '削除',
    deleting: '削除中...',
    save: '保存',
    saving: '保存中...',
    apply: '適用',
    loading: '読み込み中...',
    errorOccurred: 'エラーが発生しました',
    unknownError: '不明なエラー',
    unknownArtist: '不明なアーティスト',
    unknownAlbum: '不明なアルバム',
    unknownGenre: 'ジャンル不明',
    more: 'その他',
    play: '再生',
    playAll: 'すべて再生',
    shuffle: 'シャッフル',
    shufflePlay: 'シャッフル再生',
    playNext: '次に再生',
    addToQueue: 'キューに追加',
    albumArt: 'アルバムアート',
    off: 'オフ',
    trackCount: (count: number) => `${count}曲`,
    /** 曲数と合計時間（例: 12曲 · 45分） */
    trackCountAndDuration: (count: number, duration: string) => `${count}曲 · ${duration}`,
    /** 件数（インポート結果など） */
    itemCount: (count: number) => `${count}件`
  },

  fields: {
    title: 'タイトル',
    artist: 'アーティスト',
    album: 'アルバム',
    genre: 'ジャンル',
    year: '年',
    duration: '時間',
    rating: '評価'
  },

  format: {
    /** 合計時間（例: 1時間5分、45分） */
    totalDuration: (hours: number, minutes: number) =>
      hours > 0 ? `${hours}時間${minutes}分` : `${minutes}分`
  },

  errors: {
    /**
     * エラーコードごとの汎用メッセージ
     *
     * NOT_FOUND / VALIDATION はバックエンドのメッセージ（日本語）がユーザー向けの文言のため、
     * 日本語では含めずにそのまま表示する。
     */
    byCode: {
      LOCK: '処理が競合しています。しばらく待ってからもう一度お試しください。',
      DATABASE: 'データベースの操作中にエラーが発生しました。もう一度お試しください。',
      IO: 'ファイル操作中にエラーが発生しました。ファイルの状態を確認してください。',
      METADATA: 'メタデータの処理中にエラーが発生しました。ファイルが破損している可能性があります。'
    } as Partial<Record<AppError['code'], string>>,
    unknown: 'エラーが発生しました',
    /** エラーの通知（例: トラックの削除: ファイルが見つかりません） */
    withContext: (context: string, message: string) => `${context}: ${message}`,
    media: {
      network: 'ネットワークエラーが発生しました',
      decode: 'デコードエラー: ファイルが破損しているか未対応の形式です',
      unsupported: '未対応のフォーマットか、ファイルが見つかりません',
      generic: '再生エラーが発生しました'
    },
    playbackFailed: 'トラックの再生に失敗しました'
  },

  /** 操作の名前（エラーの通知で「操作名: 理由」の形で使う） */
  operations: {
    fetchTracks: 'トラック一覧の取得',
    searchTracks: 'トラック検索',
    filterTracks: 'トラックフィルタリング',
    fetchFavorites: 'お気に入り一覧の取得',
    fetchMostPlayed: 'よく再生するトラック一覧の取得',
    fetchRecentlyPlayed: '最近再生したトラック一覧の取得',
    fetchAlbums: 'アルバム一覧の取得',
    fetchArtists: 'アーティスト一覧の取得',
    fetchGenres: 'ジャンル一覧の取得',
    toggleFavorite: 'お気に入りの切り替え',
    setRating: 'レーティングの設定',
    deleteTracks: 'トラックの削除',
    deleteTracksAndFiles: 'トラックとファイルの削除',
    refreshMetadata: 'メタデータの更新',
    writeMetadataToFiles: 'メタデータの書き出し',
    showInFolder: 'ファイルの場所を開く',
    fetchPlaylists: 'プレイリスト一覧の取得',
    createPlaylist: 'プレイリストの作成',
    addTrackToPlaylist: 'トラックの追加',
    removeTrackFromPlaylist: 'トラックの削除',
    reorderPlaylistTracks: 'トラックの並び替え',
    renamePlaylist: 'プレイリスト名の変更',
    deletePlaylist: 'プレイリストの削除',
    loadSettings: '設定の読み込み',
    saveSettings: '設定の保存',
    openProjectPage: 'ページを開く',
    fetchLibraryFolders: 'ライブラリフォルダの取得',
    rescanLibraryFolder: '再スキャン',
    removeLibraryFolder: 'ライブラリフォルダの削除'
  },

  /** 操作の結果の通知 */
  notices: {
    playlistCreated: 'プレイリストを作成しました',
    tracksAddedToPlaylist: (count: number) => `${count}曲をプレイリストに追加しました`,
    tracksAlreadyInPlaylist: 'すでにプレイリストに入っています',
    trackRemovedFromPlaylist: 'トラックをプレイリストから削除しました',
    tracksReordered: 'トラックを並び替えました',
    playlistRenamed: 'プレイリスト名を変更しました',
    playlistDeleted: 'プレイリストを削除しました',
    tracksRemovedFromLibrary: (count: number) =>
      count === 1
        ? 'トラックをライブラリから削除しました'
        : `${count}曲をライブラリから削除しました`,
    tracksAndFilesDeleted: (count: number) =>
      count === 1 ? 'トラックとファイルを削除しました' : `${count}曲とファイルを削除しました`,
    tracksPartiallyDeleted: (deleted: number, failed: number) =>
      `${deleted}曲を削除しました（${failed}曲は削除に失敗）`,
    allTracksDeleteFailed: 'すべてのトラックの削除に失敗しました',
    metadataRefreshed: (updated: number, skipped: number, errors: number) =>
      `メタデータ更新完了\n更新: ${updated}件\nスキップ: ${skipped}件\nエラー: ${errors}件`
  },

  validation: {
    yearNotNumber: '年は数値で入力してください',
    yearOutOfRange: '年は1000から9999の範囲で指定してください',
    trackNumberNotNumber: 'トラック番号は数値で入力してください',
    trackNumberOutOfRange: 'トラック番号は1から999の範囲で指定してください',
    tooLong: (field: string, maxLength: number, length: number) =>
      `${field}は${maxLength}文字以内で入力してください（現在: ${length}文字）`,
    playlistNameRequired: 'プレイリスト名を入力してください',
    playlistNameTooLong: 'プレイリスト名は100文字以内で入力してください',
    playlistNameInvalid: 'プレイリスト名に使用できない文字が含まれています',
    inputRequired: '入力してください'
  },

  dialog: {
    confirmTitle: '確認'
  },

  player: {
    noTrack: 'トラックを選択して再生',
    shuffleTitle: 'シャッフル (S)',
    shuffle: 'シャッフル',
    previousTitle: '前へ (Ctrl+←)',
    previous: '前のトラック',
    playTitle: '再生 (Space)',
    pauseTitle: '一時停止 (Space)',
    play: '再生',
    pause: '一時停止',
    nextTitle: '次へ (Ctrl+→)',
    next: '次のトラック',
    repeatTitle: (mode: string) => `リピート (R): ${mode}`,
    repeat: 'リピート',
    repeatModes: { off: 'オフ', all: '全曲', one: '1曲' },
    seek: '再生位置',
    muteTitle: 'ミュート (M)',
    mute: 'ミュート',
    volume: '音量'
  },

  sidebar: {
    importFolder: 'フォルダをインポート',
    browse: 'ブラウズ',
    songs: '曲',
    albums: 'アルバム',
    artists: 'アーティスト',
    genres: 'ジャンル',
    expandGenres: 'ジャンルを展開',
    library: 'ライブラリ',
    recentlyPlayed: '最近再生した曲',
    mostPlayed: 'よく再生する曲',
    playlists: 'プレイリスト',
    newPlaylistTitle: '新規プレイリスト (Ctrl+N)',
    noPlaylists: 'プレイリストがありません',
    newPlaylist: '新規プレイリスト',
    playlistName: 'プレイリスト名',
    create: '作成',
    openMenu: 'メニューを開く'
  },

  rightSidebar: {
    queueTitle: '再生キュー (Q)',
    openQueue: '再生キューを開く',
    equalizerTitle: 'イコライザ (E)',
    openEqualizer: 'イコライザを開く',
    pin: 'サイドバーを固定',
    unpin: '固定解除',
    queue: '再生キュー',
    clear: 'クリア',
    nowPlaying: '再生中',
    upNext: (count: number) => `次に再生 (${count}曲)`,
    removeFromQueue: 'キューから削除',
    noUpcoming: 'キューに他のトラックはありません'
  },

  equalizer: {
    title: 'イコライザ',
    turnOff: 'イコライザをオフ',
    turnOn: 'イコライザをオン',
    custom: 'カスタム',
    builtIn: 'ビルトイン',
    saved: '保存済み',
    savePreset: 'プリセットを保存',
    savePresetHint: 'プリセットを変更すると保存可能',
    reset: 'リセット',
    resetLabel: 'イコライザをリセット',
    presetNamePlaceholder: 'プリセット名を入力',
    deletePreset: 'プリセットを削除',
    confirmDeletePreset: (name: string) => `"${name}" を削除しますか？`
  },

  metadataEditor: {
    editTitle: 'メタデータを編集',
    bulkEditTitle: (count: number) => `${count}件のトラックを一括編集`,
    bulkHint: '空欄のフィールドは変更されません。変更したいフィールドのみ入力してください。',
    bulkPartiallyFailed: (updated: number, failed: number) =>
      `${updated}曲を更新しました（${failed}曲はファイルに書き込めませんでした）`,
    bulkAllFailed: (reason: string) => `ファイルに書き込めませんでした: ${reason}`,
    unchanged: '変更しない',
    titlePlaceholder: 'タイトルを入力',
    artistPlaceholder: 'アーティストを入力',
    albumPlaceholder: 'アルバムを入力',
    genrePlaceholder: 'ジャンルを入力',
    yearPlaceholder: '例: 2023',
    writesToFile: '変更は音楽ファイルのタグに書き込まれます'
  },

  metadataExport: {
    title: 'アプリ内の編集内容・評価の書き出し',
    description:
      '以前のバージョンでアプリ内だけに保存した編集内容（タイトルなど）と評価を、音楽ファイルのタグへ書き込みます。ファイルと違う値がある曲だけを書き換え、その後ファイルを読み直します。現在は、編集と評価は常にファイルへ書き込まれるため、この操作は一度だけ行えば十分です',
    write: 'ファイルへ書き出す',
    writing: '書き出し中...',
    confirm:
      'アプリ内の編集内容・評価を、音楽ファイルのタグへ書き込みます。ファイルが書き換わります。続けますか？',
    result: (written: number, unchanged: number, skipped: number, errors: number) =>
      `書き出し完了（書き込み: ${written}曲、変更なし: ${unchanged}曲、ファイルなし: ${skipped}曲、エラー: ${errors}曲）`
  },

  contextMenu: {
    selectedCount: (count: number) => `${count}曲を選択中`,
    editMetadata: 'メタデータを編集',
    showInFolder: 'ファイルの場所を開く',
    deleteEllipsis: '削除...',
    addToPlaylist: 'プレイリストに追加',
    noPlaylists: 'プレイリストがありません',
    playGroup: (type: string) => `${type}を再生`,
    playPlaylist: 'プレイリストを再生',
    rename: '名前を変更',
    renamePlaylistTitle: 'プレイリスト名を変更',
    newPlaylistName: '新しいプレイリスト名',
    renameConfirm: '変更',
    confirmDeletePlaylist: (name: string) => `プレイリスト「${name}」を削除しますか？`
  },

  deleteDialog: {
    title: 'トラックの削除',
    /** 確認文（太字にする曲名・曲数の前後） */
    confirm: { before: '「', after: '」を削除しますか？' },
    confirmMany: { before: '', after: 'を削除しますか？' },
    fromLibrary: 'ライブラリから削除',
    fromLibraryHint: 'ファイルはそのまま残ります',
    withFiles: 'ファイルも削除',
    withFilesHint: 'この操作は取り消せません'
  },

  importDialog: {
    title: '音楽フォルダをインポート',
    selectFolderTitle: 'インポートするフォルダを選択',
    selectFolderFailed: 'フォルダの選択に失敗しました',
    folderRequired: 'フォルダを選択してください',
    invalidPath: '不正なファイルパスです',
    importFailed: (message: string) => `インポートに失敗しました: ${message}`,
    selectedFolder: '選択されたフォルダ',
    noFolder: 'フォルダが選択されていません',
    selectFolder: 'フォルダを選択',
    duplicates: '重複ファイルの処理',
    skip: 'スキップ（既存ファイルを保持）',
    replace: '置き換え（新しいファイルで上書き）',
    importing: (current: number, total: number) => `インポート中... (${current}/${total})`,
    scanning: 'スキャン中...',
    completed: 'インポート完了',
    imported: 'インポート成功:',
    skipped: 'スキップ:',
    errors: 'エラー:',
    errorDetails: 'エラー詳細:',
    importingShort: 'インポート中...',
    start: 'インポート開始'
  },

  about: {
    title: 'Muspice について',
    version: (version: string) => `バージョン ${version}`,
    tagline: 'モダンな音楽プレイヤー'
  },

  library: {
    songs: '曲',
    albums: 'アルバム',
    artists: 'アーティスト',
    genres: 'ジャンル',
    songCount: (count: number) => `${count}曲`,
    albumCount: (count: number) => `${count}枚`,
    artistCount: (count: number) => `${count}人`,
    genreCount: (count: number) => `${count}種類`,
    albumSummary: (count: number) => `${count}枚のアルバム`,
    artistSummary: (count: number) => `${count}人のアーティスト`,
    genreSummary: (count: number) => `${count}種類のジャンル`,
    searchSongs: '曲を検索...',
    searchAlbums: 'アルバムを検索...',
    searchArtists: 'アーティストを検索...',
    searchGenres: 'ジャンルを検索...',
    search: '検索...',
    clearSearch: '検索をクリア',
    selectAlbum: 'アルバムを選択してください',
    selectArtist: 'アーティストを選択してください',
    listView: 'リスト表示',
    gridView: 'グリッド表示',
    refreshMetadata: 'メタデータを更新',
    confirmRefreshMetadata:
      'すべての曲のファイルを読み直し、タグの内容（タイトル・評価など）をライブラリに反映します。ファイルへ書き出していないアプリ内の編集内容・評価は、ファイルの内容で置き換わります。続けますか？',
    loadingItems: (item: string) => `${item}を読み込み中...`,
    loadItemsFailed: (item: string) => `${item}の読み込みに失敗しました`,
    noMatch: (query: string, item: string) => `「${query}」に一致する${item}が見つかりません`,
    emptyLibrary: '音楽ライブラリが空です',
    emptyLibraryHint: 'フォルダをインポートして音楽を追加してください',
    noAlbums: 'アルバムがありません',
    noAlbumsHint: '音楽をインポートしてアルバムを追加してください',
    noArtists: 'アーティストがいません',
    noArtistsHint: '音楽をインポートしてアーティストを追加してください',
    noGenres: 'ジャンルがありません',
    noGenresHint: '音楽をインポートしてジャンルを追加してください',
    playAlbum: 'アルバムを再生',
    playArtist: 'アーティストを再生',
    playGenre: 'ジャンルを再生',
    albumsAndTracks: (albums: number, tracks: number) => `${albums}アルバム · ${tracks}曲`,
    stars: (count: number) => `${count}つ星`,
    genreList: 'ジャンル一覧',
    noTracksInGenre: 'このジャンルに曲がありません',
    noTracksInGenreHint: '他のジャンルを選択してください',
    mostPlayed: 'よく再生する曲',
    noMostPlayed: 'よく再生する曲がありません',
    noMostPlayedHint: '曲を繰り返し再生すると、ここに表示されます',
    recentlyPlayed: '最近再生した曲',
    noRecentlyPlayed: '最近再生した曲がありません',
    noRecentlyPlayedHint: '曲を再生すると、ここに表示されます'
  },

  playlists: {
    title: 'プレイリスト',
    empty: 'プレイリストがありません',
    emptyHint: 'サイドバーの「+」ボタンから新しいプレイリストを作成できます',
    confirmDelete: (name: string) =>
      `プレイリスト「${name}」を削除しますか？\nこの操作は取り消せません。`,
    notFound: 'プレイリストが見つかりません',
    notFoundHint: '選択されたプレイリストは存在しないか、削除された可能性があります',
    backToList: 'プレイリスト一覧に戻る',
    deletePlaylist: 'プレイリストを削除',
    noTracks: 'このプレイリストにはまだトラックがありません',
    noTracksHint: 'ライブラリからトラックをドラッグ&ドロップして追加できます',
    removeFromPlaylist: 'プレイリストから削除'
  },

  settings: {
    title: '設定',
    sections: {
      general: '一般',
      playback: '再生',
      library: 'ライブラリ',
      appearance: '外観'
    },
    loadFailed: '設定を読み込めませんでした',
    language: '言語',
    startupPage: '起動時に開く画面',
    startupPages: { lastOpened: '前回開いていた画面', songs: '曲一覧' },
    volumeNormalization: '音量の正規化',
    volumeNormalizations: { off: 'オフ', track: 'トラック単位', album: 'アルバム単位' },
    volumeNormalizationHint:
      '曲ごとの音量の差を、ファイルのReplayGainのタグを使ってそろえます。アルバム単位では、アルバム内の曲の音量の差はそのまま残します。タグのない曲は補正しません（既存の曲のタグは「メタデータを更新」で読み込まれます）',
    gaplessPlayback: 'ギャップレス再生',
    gaplessPlaybackHint: '次の曲を先に読み込んでおき、曲と曲の間に無音を入れずに続けて再生します',
    crossfade: 'クロスフェード',
    crossfadeSeconds: (seconds: number) => `${seconds}秒`,
    crossfadeHint:
      '曲の終わりを設定した秒数でフェードアウトしながら、次の曲をフェードインします。短い曲では曲の長さの半分までにします。「次へ」などの操作で曲を変えたときと、1曲リピートではクロスフェードしません',
    autoSync: '変更の自動反映',
    watchFolders: 'フォルダの変更を監視する',
    watchFoldersHint:
      'ライブラリフォルダでファイルが追加・削除・変更されたら、数秒後にライブラリへ反映します。外付けドライブやネットワーク上のフォルダでは、変更が通知されない場合があります',
    scanInterval: '定期的な再スキャン',
    scanIntervals: {
      0: 'しない',
      15: '15分ごと',
      30: '30分ごと',
      60: '1時間ごと',
      360: '6時間ごと'
    } as Record<number, string>,
    scanIntervalHint:
      'どちらかを有効にすると、アプリの起動時にも再スキャンします。フォルダが見つからない場合（外付けドライブが外れているなど）は、曲をライブラリから外さずに飛ばします',
    theme: 'テーマ',
    themes: { dark: 'ダーク', light: 'ライト', system: 'OSの設定に従う' },
    themeHint: '「OSの設定に従う」では、OSのダークモードの切り替えに合わせて変わります',
    accentColor: 'アクセントカラー',
    accentColorHint: 'ボタンや選択中の項目などの色に使われます',
    resetToDefault: '既定に戻す'
  },

  libraryFolders: {
    title: 'ライブラリ',
    folders: 'ライブラリフォルダ',
    foldersHint:
      'インポートしたフォルダです。再スキャンすると、フォルダ内で追加・削除・変更されたファイルをライブラリに反映します。',
    rescanAll: 'すべて再スキャン',
    progress: (current: number, total: number, file: string) =>
      `読み込み中 ${current} / ${total}: ${file}`,
    checking: 'フォルダを確認しています...',
    loadFailed: 'ライブラリフォルダを読み込めませんでした',
    empty:
      'ライブラリフォルダはまだありません。メニューの「フォルダをインポート...」でフォルダを取り込むと、ここに追加されます。',
    folderMeta: (count: number, scannedAt: string) => `${count}曲・最終スキャン: ${scannedAt}`,
    missing: 'フォルダが見つかりません',
    missingHint: (count: number) =>
      `（${count}曲。外付けドライブなどが接続されているか確認してください）`,
    rescan: '再スキャン',
    rescanning: '再スキャン中...',
    unregistered: (count: number) =>
      `どのライブラリフォルダにも含まれない曲が${count}曲あります（フォルダの記録を始める前にインポートした曲など）。同じフォルダをもう一度インポートすると、ライブラリフォルダとして登録されます（登録済みの曲は読み直しません）。`,
    removeTitle: 'ライブラリフォルダの削除',
    removeConfirm: (path: string) => `「${path}」をライブラリフォルダから外しますか？`,
    removeTracks: (count: number) => `フォルダ内の${count}曲もライブラリから外す`,
    removeTracksHint: 'ファイルは削除しません。外した曲はプレイリストからも消えます',
    removed: 'ライブラリフォルダを削除しました',
    removedWithTracks: (count: number) =>
      `ライブラリフォルダを削除し、${count}曲をライブラリから外しました`,
    readErrors: (count: number) => `${count}件のファイルを読み込めませんでした`,
    removalSkipped: (path: string) =>
      `音楽ファイルが見つからないため、ライブラリから曲を外しませんでした: ${path}`,
    rescanned: (changes: string[]) =>
      changes.length > 0
        ? `再スキャンしました（${changes.join('・')}）`
        : '再スキャンしました（変更はありませんでした）',
    added: (count: number) => `追加 ${count}曲`,
    updated: (count: number) => `更新 ${count}曲`,
    removedTracks: (count: number) => `削除 ${count}曲`
  }
};

/** メッセージの形（各言語のメッセージはこの型にする） */
export type Messages = typeof ja;
