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
    fileMissing: 'ファイルが見つかりません',
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
    addToFavorites: 'お気に入りに追加',
    removeFromFavorites: 'お気に入りから外す',
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
    rating: '評価',
    playCount: '再生回数',
    albumArtist: 'アルバムアーティスト',
    composer: '作曲者',
    grouping: 'グループ',
    comment: 'コメント',
    lyrics: '歌詞',
    trackNumber: 'トラック番号',
    trackTotal: 'トラックの総数',
    discNumber: 'ディスク番号',
    discTotal: 'ディスクの総数',
    bpm: 'BPM',
    compilation: 'コンピレーション',
    favorite: 'お気に入り',
    skipCount: 'スキップ回数',
    lastPlayedAt: '最終再生日',
    createdAt: '追加日',
    format: '形式',
    bitrate: 'ビットレート',
    sampleRate: 'サンプルレート',
    fileSize: 'サイズ',
    titleSort: 'タイトルの読み',
    artistSort: 'アーティストの読み',
    albumArtistSort: 'アルバムアーティストの読み',
    albumSort: 'アルバムの読み'
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
      METADATA:
        'メタデータの処理中にエラーが発生しました。ファイルが破損している可能性があります。',
      PLAYBACK:
        'ファイルを再生できません。対応していない形式か、ファイルが破損している、または出力デバイスを使用できない可能性があります。'
    } as Partial<Record<AppError['code'], string>>,
    unknown: 'エラーが発生しました',
    /** エラーの通知（例: トラックの削除: ファイルが見つかりません） */
    withContext: (context: string, message: string) => `${context}: ${message}`,
    playbackFailed: 'トラックの再生に失敗しました'
  },

  /** 操作の名前（エラーの通知で「操作名: 理由」の形で使う） */
  operations: {
    fetchTracks: 'トラック一覧の取得',
    searchTracks: 'トラック検索',
    filterTracks: 'トラックフィルタリング',
    fetchFavorites: 'お気に入り一覧の取得',
    fetchMostPlayed: 'よく再生するトラック一覧の取得',
    fetchPlayHistory: '再生履歴の取得',
    fetchAlbums: 'アルバム一覧の取得',
    fetchArtists: 'アーティスト一覧の取得',
    fetchGenres: 'ジャンル一覧の取得',
    setFavorite: 'お気に入りの変更',
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
    importM3u: 'プレイリストの読み込み',
    importLibraryXml: 'ライブラリのXMLの取り込み',
    exportM3u: 'プレイリストの書き出し',
    fetchAlbumArtInfo: 'アルバムアートの情報の取得',
    loadSettings: '設定の読み込み',
    loadOutputDevices: '出力デバイスの一覧の取得',
    saveSettings: '設定の保存',
    openProjectPage: 'ページを開く',
    fetchLibraryFolders: 'ライブラリフォルダの取得',
    rescanLibraryFolder: '再スキャン',
    removeLibraryFolder: 'ライブラリフォルダの削除',
    removeMissingTracks: '見つからない曲の削除',
    fetchDevices: 'デバイスの取得',
    registerDevice: 'デバイスの追加',
    updateDevice: 'デバイスの設定の変更',
    relinkDevice: '転送先の変更',
    removeDevice: 'デバイスの登録の解除',
    cancelDeviceSync: '同期の中止'
  },

  /** 操作の結果の通知 */
  notices: {
    trackFileMissing: 'ファイルが見つからないため、再生できません',
    playlistCreated: 'プレイリストを作成しました',
    tracksAddedToPlaylist: (count: number) => `${count}曲をプレイリストに追加しました`,
    tracksAlreadyInPlaylist: 'すでにプレイリストに入っています',
    trackRemovedFromPlaylist: 'トラックをプレイリストから削除しました',
    tracksReordered: 'トラックを並び替えました',
    playlistRenamed: 'プレイリスト名を変更しました',
    playlistDeleted: 'プレイリストを削除しました',
    m3uImported: (name: string, count: number) =>
      `プレイリスト「${name}」を読み込みました（${count}曲）`,
    m3uImportedMany: (count: number) => `${count}件のプレイリストを読み込みました`,
    m3uExported: (fileName: string, count: number) =>
      `「${fileName}」に書き出しました（${count}曲）`,
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
    numberOutOfRange: (field: string, min: number, max: number) =>
      `${field}は${min}から${max}の整数で指定してください`,
    playlistNameRequired: 'プレイリスト名を入力してください',
    playlistNameTooLong: 'プレイリスト名は100文字以内で入力してください',
    playlistNameInvalid: 'プレイリスト名に使用できない文字が含まれています',
    inputRequired: '入力してください'
  },

  dialog: {
    confirmTitle: '確認'
  },

  /** 曲の一覧（列を選ぶメニュー） */
  trackList: {
    columns: '表示する列',
    resetSort: '並び順を既定に戻す',
    resetColumns: '列と並び順を既定に戻す'
  },

  player: {
    noTrack: 'トラックを選択して再生',
    revealCurrentTrack: '再生中の曲を一覧で表示 (Ctrl+L)',
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
    favorites: 'お気に入り',
    playHistory: '再生履歴',
    mostPlayed: 'よく再生する曲',
    recentlyAdded: '最近追加した曲',
    folders: 'フォルダ',
    years: '年代',
    playlists: 'プレイリスト',
    newPlaylistTitle: '新規プレイリスト (Ctrl+N)',
    noPlaylists: 'プレイリストがありません',
    newPlaylist: '新規プレイリスト',
    playlistName: 'プレイリスト名',
    create: '作成',
    openMenu: 'メニューを開く',
    devices: 'デバイス',
    addDevice: 'デバイスを追加',
    noDevices: 'デバイスがありません'
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
    bulkHint:
      '空欄のフィールドは変更されません。変更したいフィールドのみ入力してください。タイトル（とその読み）・トラック番号・歌詞は、1曲ずつ編集します。',
    bulkPartiallyFailed: (updated: number, failed: number) =>
      `${updated}曲を更新しました（${failed}曲はファイルに書き込めませんでした）`,
    bulkAllFailed: (reason: string) => `ファイルに書き込めませんでした: ${reason}`,
    unchanged: '変更しない',
    titlePlaceholder: 'タイトルを入力',
    artistPlaceholder: 'アーティストを入力',
    albumPlaceholder: 'アルバムを入力',
    genrePlaceholder: 'ジャンルを入力',
    yearPlaceholder: '例: 2023',
    writesToFile: '変更は音楽ファイルのタグに書き込まれます',
    /** タブ */
    tabs: { tags: 'タグ', sort: '並び順', lyrics: '歌詞', file: 'ファイル情報' },
    /** 並び順に使う値（読み）の説明 */
    sortHint:
      '並び順に使う読み（ひらがな・カタカナなど）を入力します。空の項目は、表示用の名前で並べます。',
    /** 一括編集での、並び順の項目の見出し */
    sortHeading: '並び順（読み）',
    track: 'トラック',
    disc: 'ディスク',
    /** トラック番号・ディスク番号の、番号と総数の間の文字（例: 3 / 12） */
    of: '/',
    compilationHint: '複数のアーティストの曲を集めたアルバム',
    compilationOn: '印を付ける',
    compilationOff: '印を外す',
    lyricsPlaceholder: '歌詞を入力（ファイルに埋め込まれます）',
    loadingTags: 'ファイルのタグを読み込み中...',
    tagsUnavailable: (reason: string) => `ファイルのタグを読めないため、編集できません: ${reason}`,
    /** ファイル情報 */
    file: {
      fileName: 'ファイル名',
      location: '場所',
      format: '形式',
      bitrate: 'ビットレート',
      sampleRate: 'サンプルレート',
      duration: '長さ',
      size: 'サイズ',
      addedAt: '追加した日時',
      lastPlayedAt: '最後に再生した日時',
      playCount: '再生回数',
      skipCount: 'スキップ回数',
      never: '未再生',
      kbps: (value: number) => `${value} kbps`,
      hz: (value: number) => `${value.toLocaleString()} Hz`,
      times: (count: number) => `${count}回`,
      missing: 'ファイルが見つかりません'
    }
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

  tagTools: {
    title: (count: number) => `タグの一括ツール（${count}曲）`,
    tabs: { fileName: 'ファイル名から', renumber: '連番', replace: '検索と置換' },
    /** ファイル名から */
    pattern: '書式',
    patternPreset: '書式の例',
    patternHint:
      '使える項目: %title% %artist% %album% %albumartist% %genre% %year% %track% %disc%（読み飛ばす部分は %dummy%）。「/」で区切ると、上のフォルダの名前も使えます。',
    /** 連番 */
    renumberHint: '一覧の順に、トラック番号を振り直します。',
    startNumber: '最初の番号',
    setTotal: 'トラックの総数も書き込む',
    /** 検索と置換 */
    targetFields: '対象の項目',
    search: '検索する文字列',
    replaceWith: '置き換える文字列',
    matchCase: '大文字と小文字を区別する',
    useRegex: '正規表現',
    caseConversion: '大文字・小文字',
    caseOptions: {
      none: '変えない',
      upper: 'すべて大文字',
      lower: 'すべて小文字',
      title: '単語の先頭を大文字'
    },
    trim: '前後の空白を除く',
    /** 変更の一覧 */
    preview: '変更の一覧',
    summary: (tracks: number, fields: number) => `${tracks}曲・${fields}件の変更`,
    noChanges: '変更はありません',
    unmatched: (count: number) => `書式に合わない${count}曲は、変更しません`,
    moreChanges: (count: number) => `ほか${count}曲`,
    columnTrack: '曲',
    columnField: '項目',
    columnBefore: '前',
    columnAfter: '後',
    /** 値がない・取り除くことを表す */
    empty: '（なし）',
    unknown: '—',
    apply: '適用',
    applying: '書き込み中...',
    applied: (count: number) => `${count}曲のタグを書き換えました`,
    partiallyFailed: (updated: number, failed: number) =>
      `${updated}曲を書き換えました（${failed}曲はファイルに書き込めませんでした）`,
    allFailed: (reason: string) => `ファイルに書き込めませんでした: ${reason}`,
    errors: {
      emptyPattern: '書式を入力してください',
      noPlaceholder: '書式に、項目（%title% など）を入れてください',
      unknownPlaceholder: (name: string) => `使えない項目です: ${name}`,
      duplicatePlaceholder: (name: string) => `同じ項目は1回だけ使えます: ${name}`,
      adjacentPlaceholders: (name: string) =>
        `項目の間に、区切りの文字を入れてください: ${name} の前`,
      invalidRegex: '正規表現が正しくありません',
      numberOutOfRange: 'トラック番号は1から999の範囲にしてください',
      noFields: '対象の項目を選んでください',
      tooLong: '変更の後の値が長すぎます'
    }
  },

  albumArtDialog: {
    title: 'アルバムアート',
    /** アルバムの絵をクリックして開く操作の名前 */
    open: 'アルバムアートを変更',
    target: (count: number) => `${count}曲が対象`,
    /** 複数の曲を選んだ時の、表示している画像の説明 */
    previewOf: (title: string) => `表示しているのは「${title}」の画像です`,
    embedded: 'ファイルに埋め込まれた画像',
    folder: (fileName: string) => `フォルダの画像（${fileName}）`,
    none: '画像はありません',
    dimensions: (width: number, height: number) => `${width} × ${height}`,
    folderHint:
      'フォルダの画像は、埋め込みの画像がない曲に表示されます。画像を埋め込むと、そちらが優先されます。',
    choose: '画像を選んで埋め込む...',
    remove: '埋め込みの画像を取り除く',
    writesToFile: '画像（JPEG・PNG、10MBまで）は、縮小せずにそのまま音楽ファイルへ埋め込まれます。',
    confirmRemove: (count: number) =>
      `${count}曲のファイルから、埋め込みの画像をすべて取り除きます。元に戻せません。`,
    confirmRemoveAction: '取り除く',
    working: 'ファイルに書き込み中...',
    missingExcluded: (count: number) => `ファイルが見つからない${count}曲は、対象になりません`,
    allMissing: 'ファイルが見つからないため、変更できません',
    embeddedResult: (count: number) => `${count}曲に画像を埋め込みました`,
    removedResult: (count: number) => `${count}曲から画像を取り除きました`,
    nothingToRemove: '埋め込みの画像がある曲はありませんでした',
    partiallyFailed: (failed: number) => `${failed}曲はファイルに書き込めませんでした`,
    allFailed: (reason: string) => `ファイルに書き込めませんでした: ${reason}`
  },

  libraryXmlImport: {
    title: 'ほかのプレーヤーからの取り込み',
    description:
      'MusicBee・iTunes / ミュージックが書き出すライブラリのXML（iTunes形式）から、ファイルのタグには入っていない再生回数・最後に再生した日時・追加した日時・お気に入りを取り込みます。曲は、ファイル名とその上のフォルダの名前で対応を付けるため、先に曲をライブラリへ取り込んでおいてください',
    rules:
      '再生回数・スキップ回数は多い方、最後に再生した日時は新しい方、追加した日時は古い方にします（何度取り込んでも、増え続けません）',
    musicBeeHint:
      'MusicBeeでは、設定の「ライブラリ」にある、ライブラリをiTunes形式のXMLで書き出す項目を有効にすると、ライブラリのファイルと同じフォルダにXMLが作られます',
    includePlaylists: 'プレイリストも取り込む（同じ名前があれば、番号を付けた名前で作ります）',
    choose: 'XMLを選んで取り込む...',
    importing: '取り込み中...',
    result: (updated: number, matched: number, total: number) =>
      `取り込み完了: ${total}曲のうち${matched}曲がライブラリの曲と対応し、${updated}曲の値を更新しました`,
    playlists: (created: number, skipped: number) =>
      skipped > 0
        ? `プレイリスト: ${created}件を作成（曲が入らない${skipped}件は作成していません）`
        : `プレイリスト: ${created}件を作成`,
    unmatched: (count: number) => `ライブラリの曲と対応が付かなかった曲: ${count}曲`,
    unmatchedMore: (count: number) => `ほか${count}曲`
  },

  contextMenu: {
    selectedCount: (count: number) => `${count}曲を選択中`,
    editMetadata: 'メタデータを編集',
    albumArt: 'アルバムアート...',
    tagTools: 'タグの一括ツール...',
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
    relinked: '移動したファイルの引き継ぎ:',
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
    /** ビュー（最近追加した曲・フォルダ別・年代別） */
    recentlyAdded: '最近追加した曲',
    folders: 'フォルダ',
    selectFolder: 'フォルダを選んでください',
    years: '年代',
    selectYear: '年代か年を選んでください',
    decade: (decade: number) => `${decade}年代`,
    unknownYear: '年不明',
    /** カラムブラウザ（ジャンル / アーティスト / アルバムでの絞り込み） */
    columnBrowser: 'カラムブラウザ',
    browserAll: (count: number) => `すべて（${count}）`,
    filters: {
      button: 'フィルタ',
      anyRating: 'すべて',
      ratingAtLeast: (rating: number) => `${'★'.repeat(rating)} 以上`,
      yearFrom: '開始',
      yearTo: '終了',
      favoritesOnly: 'お気に入りのみ',
      clear: 'クリア'
    },
    /** 絞り込み（検索・フィルタ・カラムブラウザ）に合う曲がない時 */
    noMatches: '条件に合う曲がありません',
    noMatchesHint: '検索・フィルタ・カラムブラウザの条件を変えてください',
    clearFilters: '絞り込みを解除',
    filteredCount: (count: number, total: number) => `${count} / ${total}曲`,
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
    noMostPlayedHint: '曲の半分か4分を聴くと、1回と数えます',
    favorites: 'お気に入り',
    noFavorites: 'お気に入りの曲がありません',
    noFavoritesHint: '曲のハートを押すと、ここに表示されます',
    playHistory: '再生履歴',
    noPlayHistory: '再生履歴がありません',
    noPlayHistoryHint: '曲の半分か4分を聴くと、ここに記録されます',
    /** 再生履歴の日付の見出し */
    today: '今日',
    yesterday: '昨日',
    /** 再生履歴の、再生した時刻の列 */
    playedAt: '時刻'
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
    removeFromPlaylist: 'プレイリストから削除',
    importM3u: 'M3Uを読み込む',
    exportM3u: 'M3U8で書き出す...',
    importResultTitle: 'プレイリストの読み込み結果',
    /** 読み込んだファイルごとの結果 */
    importCreated: (name: string, count: number) => `プレイリスト「${name}」を作成（${count}曲）`,
    importNotCreated: 'ライブラリの曲と対応する行がなかったため、プレイリストを作りませんでした',
    importFailed: (message: string) => `読み込めませんでした: ${message}`,
    importDuplicates: (count: number) =>
      `同じ曲が${count}回重ねて書かれていたため、1回だけ入れました`,
    importUnmatched: (count: number) => `ライブラリの曲と対応が付かなかった行: ${count}件`,
    importUnmatchedMore: (count: number) => `ほか${count}件`,
    importUnmatchedHint:
      '曲のファイル名と、その上のフォルダの名前で対応を付けます。曲をライブラリに取り込んでから、もう一度読み込んでください。',
    exportTitle: 'プレイリストを書き出す',
    exportDescription: (name: string) => `「${name}」を、M3U8（UTF-8）のファイルへ書き出します。`,
    exportPathStyle: '曲の場所の書き方',
    exportAbsolute: '絶対パス',
    exportAbsoluteHint:
      'このMacの中での場所をそのまま書きます。このMacのほかのアプリで開く場合に向いています。',
    exportRelative: '書き出し先からの相対パス',
    exportRelativeHint:
      'プレイリストのファイルから見た場所で書きます。曲と一緒に別の場所・機器へ移す場合に向いています。',
    exportConfirm: '保存先を選ぶ...'
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
    sidebarItems: 'サイドバーに表示する項目',
    sidebarItemsHint: '印を外した項目は、サイドバーに出しません',
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
    outputDevice: '出力デバイス',
    outputDeviceDefault: 'システムの既定',
    /** 選んであるデバイスが一覧にない（接続されていない）場合の表示 */
    outputDeviceDisconnected: '接続されていないデバイス',
    outputDeviceHint:
      '音を出すデバイスです。選んだデバイスが接続されていない間は、システムの既定のデバイスで再生します。再生中に変えると、同じ位置から続けて再生します',
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
    missingSkipped: (path: string) =>
      `音楽ファイルが1件も見つからないため、曲の状態は変えませんでした: ${path}`,
    missingTracks: (count: number) =>
      `ファイルが見つからない曲が${count}曲あります。移動・改名したファイルは、再スキャン（または移動先のフォルダのインポート）で見つかると、同じ曲として引き継がれます（お気に入り・再生回数・プレイリストを保ちます）。`,
    removeMissing: '見つからない曲を外す',
    removeMissingTitle: '見つからない曲の削除',
    removeMissingConfirm: (count: number) =>
      `ファイルが見つからない${count}曲を、ライブラリから外しますか？`,
    removeMissingHint:
      '外した曲はプレイリストからも消え、お気に入り・再生回数も失われます。ファイルを移動・改名しただけの場合は、外さずに再スキャンしてください',
    missingRemoved: (count: number) => `見つからない曲を${count}曲、ライブラリから外しました`,
    rescanned: (changes: string[]) =>
      changes.length > 0
        ? `再スキャンしました（${changes.join('・')}）`
        : '再スキャンしました（変更はありませんでした）',
    added: (count: number) => `追加 ${count}曲`,
    updated: (count: number) => `更新 ${count}曲`,
    relinked: (count: number) => `移動 ${count}曲`,
    markedMissing: (count: number) => `見つからない ${count}曲`,
    restored: (count: number) => `見つかった ${count}曲`
  },

  devices: {
    selectFolderTitle: '転送先のフォルダを選択',
    selectFolderFailed: 'フォルダの選択に失敗しました',
    addTitle: 'デバイスを追加',
    nameLabel: 'デバイス名',
    add: '追加',
    nameTooLong: 'デバイス名が長すぎます',
    added: (name: string) => `デバイス「${name}」を追加しました`,
    notFound: 'デバイスが見つかりません',
    notFoundHint: '選択されたデバイスは存在しないか、登録を解除された可能性があります',
    connected: '接続中',
    disconnected: '未接続',
    disconnectedHint:
      '転送先のフォルダが見つかりません。デバイスを接続してください。フォルダの場所が変わった場合は「転送先を変更」で選び直せます',
    freeSpace: (free: string, total: string) => `空き ${free} / ${total}`,
    lastSynced: (at: string) => `最終同期: ${at}`,
    neverSynced: 'まだ同期していません',
    sync: '同期',
    rename: '名前を変更',
    renameTitle: 'デバイス名を変更',
    renameConfirm: '変更',
    relink: '転送先を変更',
    relinked: '転送先を変更しました',
    remove: '登録を解除',
    confirmRemove: (name: string) =>
      `デバイス「${name}」の登録を解除しますか？\nデバイス上の曲は削除されません。`,
    removed: 'デバイスの登録を解除しました',
    sourceTitle: '同期する内容',
    layoutHint: '曲は「アーティスト/アルバム/曲名」のフォルダに分けてコピーします',
    syncAll: 'ライブラリの全曲',
    playlists: 'プレイリスト',
    playlistsHint:
      '選んだプレイリストは、デバイスで開ける.m3u8ファイルとしても書き出します。「ライブラリの全曲」を選ばない場合は、選んだプレイリストの曲だけをコピーします',
    noPlaylists: 'プレイリストがありません',
    removeUnselected: '同期する内容から外れた曲を、デバイスから削除する',
    removeUnselectedHint:
      '削除するのはMuspiceがコピーした曲だけです。自分でデバイスに置いたファイルは削除しません',
    noSource: '同期する内容を選択してください',

    syncDialog: {
      title: (name: string) => `「${name}」へ同期`,
      planning: '差分を確認しています...',
      planFailed: (message: string) => `差分を確認できませんでした: ${message}`,
      copy: 'コピー',
      delete: '削除',
      rename: '名前の変更',
      unchanged: '変更なし',
      playlists: 'プレイリスト',
      /** 曲数とサイズ（例: 12曲（345.6 MB）） */
      countAndSize: (count: number, size: string) => `${count}曲（${size}）`,
      freeSpace: '空き容量',
      upToDate: 'デバイスの曲は最新です',
      missingSources: (count: number) =>
        `元のファイルが見つからない曲が${count}曲あります（コピーできません。コピー済みの曲はデバイスに残します）`,
      notEnoughSpace: (size: string) => `デバイスの空き容量が足りません（あと${size}必要です）`,
      start: '同期を開始',
      preparing: '準備しています...',
      copying: (current: number, total: number) => `コピー中... (${current}/${total})`,
      finishing: '仕上げています...',
      stop: '中止',
      stopping: '中止しています...',
      failed: (message: string) => `同期に失敗しました: ${message}`,
      completed: '同期が完了しました',
      cancelled: '同期を中止しました',
      cancelledHint: 'コピー済みの曲はデバイスに残ります。もう一度同期すると、続きからコピーします',
      copied: 'コピー:',
      deleted: '削除:',
      renamed: '名前の変更:',
      playlistsWritten: 'プレイリスト:',
      errors: 'エラー:',
      errorDetails: 'エラー詳細:'
    }
  }
};

/** メッセージの形（各言語のメッセージはこの型にする） */
export type Messages = typeof ja;
