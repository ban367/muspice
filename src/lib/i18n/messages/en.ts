/**
 * 英語のメッセージ（形は日本語（`ja.ts`）の`Messages`型に合わせる）
 */
import type { Messages } from './ja.js';

/** 数に合わせて単数形・複数形を選ぶ */
function plural(count: number, one: string, other: string): string {
  return `${count} ${count === 1 ? one : other}`;
}

export const en: Messages = {
  common: {
    cancel: 'Cancel',
    close: 'Close',
    delete: 'Delete',
    deleting: 'Deleting...',
    save: 'Save',
    saving: 'Saving...',
    apply: 'Apply',
    loading: 'Loading...',
    errorOccurred: 'An error occurred',
    unknownError: 'Unknown error',
    unknownArtist: 'Unknown Artist',
    unknownAlbum: 'Unknown Album',
    unknownGenre: 'Unknown Genre',
    fileMissing: 'File not found',
    more: 'More',
    play: 'Play',
    playAll: 'Play All',
    shuffle: 'Shuffle',
    shufflePlay: 'Shuffle Play',
    playNext: 'Play Next',
    addToQueue: 'Add to Queue',
    albumArt: 'Album art',
    off: 'Off',
    trackCount: (count) => plural(count, 'track', 'tracks'),
    trackCountAndDuration: (count, duration) => `${plural(count, 'track', 'tracks')} · ${duration}`,
    itemCount: (count) => `${count}`
  },

  fields: {
    title: 'Title',
    artist: 'Artist',
    album: 'Album',
    genre: 'Genre',
    year: 'Year',
    duration: 'Time',
    rating: 'Rating'
  },

  format: {
    totalDuration: (hours, minutes) => (hours > 0 ? `${hours} hr ${minutes} min` : `${minutes} min`)
  },

  errors: {
    // バックエンドのメッセージは日本語のため、英語ではすべてのコードを汎用メッセージにする
    byCode: {
      NOT_FOUND: 'The item could not be found. It may have been moved or deleted.',
      VALIDATION: 'The input is invalid. Please check the values and try again.',
      LOCK: 'Another operation is in progress. Please wait a moment and try again.',
      DATABASE: 'A database error occurred. Please try again.',
      IO: 'A file error occurred. Please check the file.',
      METADATA: 'Failed to process the metadata. The file may be corrupted.',
      PLAYBACK:
        'This file could not be played. The format may be unsupported, the file may be corrupted, or the output device may be unavailable.'
    },
    unknown: 'An error occurred',
    withContext: (context, message) => `${context}: ${message}`,
    playbackFailed: 'Failed to play the track'
  },

  operations: {
    fetchTracks: 'Loading tracks',
    searchTracks: 'Searching tracks',
    filterTracks: 'Filtering tracks',
    fetchFavorites: 'Loading favorites',
    fetchMostPlayed: 'Loading most played tracks',
    fetchRecentlyPlayed: 'Loading recently played tracks',
    fetchAlbums: 'Loading albums',
    fetchArtists: 'Loading artists',
    fetchGenres: 'Loading genres',
    toggleFavorite: 'Updating favorite',
    setRating: 'Setting rating',
    deleteTracks: 'Deleting tracks',
    deleteTracksAndFiles: 'Deleting tracks and files',
    refreshMetadata: 'Refreshing metadata',
    writeMetadataToFiles: 'Writing metadata to files',
    showInFolder: 'Showing the file',
    fetchPlaylists: 'Loading playlists',
    createPlaylist: 'Creating the playlist',
    addTrackToPlaylist: 'Adding the track',
    removeTrackFromPlaylist: 'Removing the track',
    reorderPlaylistTracks: 'Reordering tracks',
    renamePlaylist: 'Renaming the playlist',
    deletePlaylist: 'Deleting the playlist',
    loadSettings: 'Loading settings',
    loadOutputDevices: 'Loading output devices',
    saveSettings: 'Saving settings',
    openProjectPage: 'Opening the page',
    fetchLibraryFolders: 'Loading library folders',
    rescanLibraryFolder: 'Rescanning',
    removeLibraryFolder: 'Removing the library folder',
    removeMissingTracks: 'Removing missing tracks',
    fetchDevices: 'Loading devices',
    registerDevice: 'Adding the device',
    updateDevice: 'Changing the device settings',
    relinkDevice: 'Changing the destination',
    removeDevice: 'Removing the device',
    cancelDeviceSync: 'Stopping the sync'
  },

  notices: {
    trackFileMissing: 'This track cannot be played because its file was not found',
    playlistCreated: 'Playlist created',
    tracksAddedToPlaylist: (count) => `Added ${plural(count, 'track', 'tracks')} to the playlist`,
    tracksAlreadyInPlaylist: 'Already in the playlist',
    trackRemovedFromPlaylist: 'Removed the track from the playlist',
    tracksReordered: 'Reordered the tracks',
    playlistRenamed: 'Playlist renamed',
    playlistDeleted: 'Playlist deleted',
    tracksRemovedFromLibrary: (count) =>
      count === 1
        ? 'Removed the track from the library'
        : `Removed ${count} tracks from the library`,
    tracksAndFilesDeleted: (count) =>
      count === 1 ? 'Deleted the track and its file' : `Deleted ${count} tracks and their files`,
    tracksPartiallyDeleted: (deleted, failed) =>
      `Deleted ${plural(deleted, 'track', 'tracks')} (${failed} could not be deleted)`,
    allTracksDeleteFailed: 'Failed to delete all of the tracks',
    metadataRefreshed: (updated, skipped, errors) =>
      `Metadata refreshed\nUpdated: ${updated}\nSkipped: ${skipped}\nErrors: ${errors}`
  },

  validation: {
    yearNotNumber: 'Enter the year as a number',
    yearOutOfRange: 'Enter a year between 1000 and 9999',
    trackNumberNotNumber: 'Enter the track number as a number',
    trackNumberOutOfRange: 'Enter a track number between 1 and 999',
    tooLong: (field, maxLength, length) =>
      `${field} must be ${maxLength} characters or fewer (currently ${length})`,
    playlistNameRequired: 'Enter a playlist name',
    playlistNameTooLong: 'The playlist name must be 100 characters or fewer',
    playlistNameInvalid: 'The playlist name contains characters that cannot be used',
    inputRequired: 'Enter a value'
  },

  dialog: {
    confirmTitle: 'Confirm'
  },

  player: {
    noTrack: 'Select a track to play',
    shuffleTitle: 'Shuffle (S)',
    shuffle: 'Shuffle',
    previousTitle: 'Previous (Ctrl+←)',
    previous: 'Previous track',
    playTitle: 'Play (Space)',
    pauseTitle: 'Pause (Space)',
    play: 'Play',
    pause: 'Pause',
    nextTitle: 'Next (Ctrl+→)',
    next: 'Next track',
    repeatTitle: (mode) => `Repeat (R): ${mode}`,
    repeat: 'Repeat',
    repeatModes: { off: 'Off', all: 'All', one: 'One' },
    seek: 'Playback position',
    muteTitle: 'Mute (M)',
    mute: 'Mute',
    volume: 'Volume'
  },

  sidebar: {
    importFolder: 'Import Folder',
    browse: 'Browse',
    songs: 'Songs',
    albums: 'Albums',
    artists: 'Artists',
    genres: 'Genres',
    expandGenres: 'Expand genres',
    library: 'Library',
    recentlyPlayed: 'Recently Played',
    mostPlayed: 'Most Played',
    playlists: 'Playlists',
    newPlaylistTitle: 'New Playlist (Ctrl+N)',
    noPlaylists: 'No playlists',
    newPlaylist: 'New Playlist',
    playlistName: 'Playlist name',
    create: 'Create',
    openMenu: 'Open menu',
    devices: 'Devices',
    addDevice: 'Add Device',
    noDevices: 'No devices'
  },

  rightSidebar: {
    queueTitle: 'Queue (Q)',
    openQueue: 'Open the queue',
    equalizerTitle: 'Equalizer (E)',
    openEqualizer: 'Open the equalizer',
    pin: 'Pin the sidebar',
    unpin: 'Unpin',
    queue: 'Queue',
    clear: 'Clear',
    nowPlaying: 'Now Playing',
    upNext: (count) => `Up Next (${plural(count, 'track', 'tracks')})`,
    removeFromQueue: 'Remove from queue',
    noUpcoming: 'No other tracks in the queue'
  },

  equalizer: {
    title: 'Equalizer',
    turnOff: 'Turn off the equalizer',
    turnOn: 'Turn on the equalizer',
    custom: 'Custom',
    builtIn: 'Built-in',
    saved: 'Saved',
    savePreset: 'Save preset',
    savePresetHint: 'Change the preset to save it',
    reset: 'Reset',
    resetLabel: 'Reset the equalizer',
    presetNamePlaceholder: 'Preset name',
    deletePreset: 'Delete preset',
    confirmDeletePreset: (name) => `Delete "${name}"?`
  },

  metadataEditor: {
    editTitle: 'Edit Metadata',
    bulkEditTitle: (count) => `Edit ${plural(count, 'track', 'tracks')}`,
    bulkHint: 'Empty fields are left unchanged. Fill in only the fields you want to change.',
    bulkPartiallyFailed: (updated, failed) =>
      `Updated ${plural(updated, 'track', 'tracks')} (${failed} could not be written to the file)`,
    bulkAllFailed: (reason) => `Could not write to the files: ${reason}`,
    unchanged: 'Unchanged',
    titlePlaceholder: 'Title',
    artistPlaceholder: 'Artist',
    albumPlaceholder: 'Album',
    genrePlaceholder: 'Genre',
    yearPlaceholder: 'e.g. 2023',
    writesToFile: 'Changes are written to the tags of the music files'
  },

  metadataExport: {
    title: 'Write app-only edits and ratings to files',
    description:
      'Writes the edits (title and so on) and ratings that earlier versions saved only in the app to the tags of the music files. Only tracks whose values differ from the file are rewritten, and the files are reread afterwards. Edits and ratings are now always written to the files, so this only needs to be done once',
    write: 'Write to files',
    writing: 'Writing...',
    confirm:
      'This writes the edits and ratings saved in the app to the tags of the music files. The files will be modified. Continue?',
    result: (written, unchanged, skipped, errors) =>
      `Finished writing (written: ${written}, unchanged: ${unchanged}, missing files: ${skipped}, errors: ${errors})`
  },

  contextMenu: {
    selectedCount: (count) => `${plural(count, 'track', 'tracks')} selected`,
    editMetadata: 'Edit Metadata',
    showInFolder: 'Show in Folder',
    deleteEllipsis: 'Delete...',
    addToPlaylist: 'Add to Playlist',
    noPlaylists: 'No playlists',
    playGroup: (type) => `Play ${type}`,
    playPlaylist: 'Play Playlist',
    rename: 'Rename',
    renamePlaylistTitle: 'Rename Playlist',
    newPlaylistName: 'New playlist name',
    renameConfirm: 'Rename',
    confirmDeletePlaylist: (name) => `Delete the playlist "${name}"?`
  },

  deleteDialog: {
    title: 'Delete Tracks',
    confirm: { before: 'Delete "', after: '"?' },
    confirmMany: { before: 'Delete ', after: '?' },
    fromLibrary: 'Remove from library',
    fromLibraryHint: 'The files are kept',
    withFiles: 'Also delete the files',
    withFilesHint: 'This cannot be undone'
  },

  importDialog: {
    title: 'Import Music Folder',
    selectFolderTitle: 'Select a folder to import',
    selectFolderFailed: 'Failed to select the folder',
    folderRequired: 'Select a folder',
    invalidPath: 'The file path is invalid',
    importFailed: (message) => `Import failed: ${message}`,
    selectedFolder: 'Selected folder',
    noFolder: 'No folder selected',
    selectFolder: 'Select Folder',
    duplicates: 'Duplicate files',
    skip: 'Skip (keep existing files)',
    replace: 'Replace (overwrite with the new files)',
    importing: (current, total) => `Importing... (${current}/${total})`,
    scanning: 'Scanning...',
    completed: 'Import Complete',
    imported: 'Imported:',
    skipped: 'Skipped:',
    relinked: 'Relinked moved files:',
    errors: 'Errors:',
    errorDetails: 'Error details:',
    importingShort: 'Importing...',
    start: 'Start Import'
  },

  about: {
    title: 'About Muspice',
    version: (version) => `Version ${version}`,
    tagline: 'A modern music player'
  },

  library: {
    songs: 'Songs',
    albums: 'Albums',
    artists: 'Artists',
    genres: 'Genres',
    songCount: (count) => plural(count, 'song', 'songs'),
    albumCount: (count) => plural(count, 'album', 'albums'),
    artistCount: (count) => plural(count, 'artist', 'artists'),
    genreCount: (count) => plural(count, 'genre', 'genres'),
    albumSummary: (count) => plural(count, 'album', 'albums'),
    artistSummary: (count) => plural(count, 'artist', 'artists'),
    genreSummary: (count) => plural(count, 'genre', 'genres'),
    searchSongs: 'Search songs...',
    searchAlbums: 'Search albums...',
    searchArtists: 'Search artists...',
    searchGenres: 'Search genres...',
    search: 'Search...',
    clearSearch: 'Clear search',
    selectAlbum: 'Select an album',
    selectArtist: 'Select an artist',
    listView: 'List view',
    gridView: 'Grid view',
    refreshMetadata: 'Refresh metadata',
    confirmRefreshMetadata:
      'This rereads every file and updates the library with its tags (title, rating and so on). Edits and ratings that were saved only in the app and not written to the files are replaced by the contents of the files. Continue?',
    loadingItems: (item) => `Loading ${item.toLowerCase()}...`,
    loadItemsFailed: (item) => `Failed to load ${item.toLowerCase()}`,
    noMatch: (query, item) => `No ${item.toLowerCase()} match "${query}"`,
    emptyLibrary: 'Your music library is empty',
    emptyLibraryHint: 'Import a folder to add music',
    noAlbums: 'No albums',
    noAlbumsHint: 'Import music to add albums',
    noArtists: 'No artists',
    noArtistsHint: 'Import music to add artists',
    noGenres: 'No genres',
    noGenresHint: 'Import music to add genres',
    playAlbum: 'Play album',
    playArtist: 'Play artist',
    playGenre: 'Play genre',
    albumsAndTracks: (albums, tracks) =>
      `${plural(albums, 'album', 'albums')} · ${plural(tracks, 'track', 'tracks')}`,
    stars: (count) => plural(count, 'star', 'stars'),
    genreList: 'Genres',
    noTracksInGenre: 'No songs in this genre',
    noTracksInGenreHint: 'Select another genre',
    mostPlayed: 'Most Played',
    noMostPlayed: 'No most played songs yet',
    noMostPlayedHint: 'Songs you play often will appear here',
    recentlyPlayed: 'Recently Played',
    noRecentlyPlayed: 'No recently played songs',
    noRecentlyPlayedHint: 'Songs you play will appear here'
  },

  playlists: {
    title: 'Playlists',
    empty: 'No playlists',
    emptyHint: 'Create a playlist with the "+" button in the sidebar',
    confirmDelete: (name) => `Delete the playlist "${name}"?\nThis cannot be undone.`,
    notFound: 'Playlist not found',
    notFoundHint: 'The playlist does not exist or may have been deleted',
    backToList: 'Back to playlists',
    deletePlaylist: 'Delete playlist',
    noTracks: 'This playlist has no tracks yet',
    noTracksHint: 'Drag and drop tracks from the library to add them',
    removeFromPlaylist: 'Remove from playlist'
  },

  settings: {
    title: 'Settings',
    sections: {
      general: 'General',
      playback: 'Playback',
      library: 'Library',
      appearance: 'Appearance'
    },
    loadFailed: 'Could not load the settings',
    language: 'Language',
    startupPage: 'Open at startup',
    startupPages: { lastOpened: 'The last opened page', songs: 'Songs' },
    volumeNormalization: 'Volume normalization',
    volumeNormalizations: { off: 'Off', track: 'Per track', album: 'Per album' },
    volumeNormalizationHint:
      'Evens out the loudness of tracks using the ReplayGain tags in the files. Per album keeps the differences between tracks on the same album. Tracks without tags are not adjusted (run "Refresh metadata" to read the tags of existing tracks).',
    gaplessPlayback: 'Gapless playback',
    gaplessPlaybackHint:
      'Loads the next track in advance and plays it without silence between tracks.',
    crossfade: 'Crossfade',
    crossfadeSeconds: (seconds) => `${seconds} s`,
    crossfadeHint:
      'Fades out the end of a track over the set time while fading in the next one. Short tracks are crossfaded for at most half of their length. Tracks changed with "Next" and the like, and repeat one, are not crossfaded.',
    outputDevice: 'Output device',
    outputDeviceDefault: 'System default',
    outputDeviceDisconnected: 'Disconnected device',
    outputDeviceHint:
      'The device that plays the sound. While the selected device is disconnected, the system default device is used. Changing it during playback continues from the same position.',
    autoSync: 'Automatic updates',
    watchFolders: 'Watch folders for changes',
    watchFoldersHint:
      'When files are added, deleted or changed in a library folder, the library is updated a few seconds later. Changes on external or network drives may not be notified.',
    scanInterval: 'Rescan periodically',
    scanIntervals: {
      0: 'Never',
      15: 'Every 15 minutes',
      30: 'Every 30 minutes',
      60: 'Every hour',
      360: 'Every 6 hours'
    },
    scanIntervalHint:
      'With either option on, the folders are also rescanned at startup. Missing folders (such as a disconnected external drive) are skipped without removing their tracks from the library.',
    theme: 'Theme',
    themes: { dark: 'Dark', light: 'Light', system: 'Use the system setting' },
    themeHint: 'With "Use the system setting", the theme follows the dark mode of the OS.',
    accentColor: 'Accent color',
    accentColorHint: 'Used for buttons, selected items and so on.',
    resetToDefault: 'Reset to Default'
  },

  libraryFolders: {
    title: 'Library',
    folders: 'Library folders',
    foldersHint:
      'Folders you have imported. Rescanning updates the library with the files added, deleted or changed in the folder.',
    rescanAll: 'Rescan All',
    progress: (current, total, file) => `Loading ${current} / ${total}: ${file}`,
    checking: 'Checking the folders...',
    loadFailed: 'Could not load the library folders',
    empty:
      'There are no library folders yet. Folders you import with "Import Folder..." in the menu appear here.',
    folderMeta: (count, scannedAt) =>
      `${plural(count, 'track', 'tracks')} · Last scan: ${scannedAt}`,
    missing: 'Folder not found',
    missingHint: (count) =>
      ` (${plural(count, 'track', 'tracks')}. Check that the external drive or the like is connected.)`,
    rescan: 'Rescan',
    rescanning: 'Rescanning...',
    unregistered: (count) =>
      `${plural(count, 'track is', 'tracks are')} not in any library folder (such as tracks imported before folders were recorded). Import the same folder again to register it as a library folder (registered tracks are not read again).`,
    removeTitle: 'Remove Library Folder',
    removeConfirm: (path) => `Remove "${path}" from the library folders?`,
    removeTracks: (count) =>
      `Also remove the ${plural(count, 'track', 'tracks')} in the folder from the library`,
    removeTracksHint:
      'The files are not deleted. The removed tracks are also removed from playlists.',
    removed: 'Removed the library folder',
    removedWithTracks: (count) =>
      `Removed the library folder and ${plural(count, 'track', 'tracks')} from the library`,
    readErrors: (count) => `Could not read ${plural(count, 'file', 'files')}`,
    missingSkipped: (path) =>
      `No music files were found, so the tracks were left as they are: ${path}`,
    missingTracks: (count) =>
      `The ${plural(count, 'file of a track was', 'files of tracks were')} not found. When a moved or renamed file is found by a rescan (or by importing the folder it was moved to), it is kept as the same track (favorites, play counts and playlists are preserved).`,
    removeMissing: 'Remove Missing Tracks',
    removeMissingTitle: 'Remove Missing Tracks',
    removeMissingConfirm: (count) =>
      `Remove the ${plural(count, 'track', 'tracks')} whose files were not found from the library?`,
    removeMissingHint:
      'The removed tracks are also removed from playlists, and their favorites and play counts are lost. If you only moved or renamed the files, rescan instead of removing.',
    missingRemoved: (count) =>
      `Removed ${plural(count, 'missing track', 'missing tracks')} from the library`,
    rescanned: (changes) =>
      changes.length > 0 ? `Rescanned (${changes.join(', ')})` : 'Rescanned (no changes)',
    added: (count) => `${count} added`,
    updated: (count) => `${count} updated`,
    relinked: (count) => `${count} moved`,
    markedMissing: (count) => `${count} missing`,
    restored: (count) => `${count} found again`
  },

  devices: {
    selectFolderTitle: 'Select the destination folder',
    selectFolderFailed: 'Failed to select a folder',
    addTitle: 'Add Device',
    nameLabel: 'Device name',
    add: 'Add',
    nameTooLong: 'The device name is too long',
    added: (name) => `Added the device "${name}"`,
    notFound: 'Device not found',
    notFoundHint: 'The selected device does not exist or may have been removed',
    connected: 'Connected',
    disconnected: 'Not connected',
    disconnectedHint:
      'The destination folder was not found. Connect the device. If the folder has moved, choose it again with "Change Destination".',
    freeSpace: (free, total) => `${free} free of ${total}`,
    lastSynced: (at) => `Last sync: ${at}`,
    neverSynced: 'Not synced yet',
    sync: 'Sync',
    rename: 'Rename',
    renameTitle: 'Rename Device',
    renameConfirm: 'Rename',
    relink: 'Change Destination',
    relinked: 'Changed the destination',
    remove: 'Remove Device',
    confirmRemove: (name) =>
      `Remove the device "${name}"?\nThe tracks on the device are not deleted.`,
    removed: 'Removed the device',
    sourceTitle: 'What to sync',
    layoutHint: 'Tracks are copied into "Artist/Album/Title" folders',
    syncAll: 'All tracks in the library',
    playlists: 'Playlists',
    playlistsHint:
      'The selected playlists are also written as .m3u8 files that the device can open. Unless "All tracks in the library" is selected, only the tracks in the selected playlists are copied.',
    noPlaylists: 'No playlists',
    removeUnselected: 'Delete tracks that are no longer selected from the device',
    removeUnselectedHint:
      'Only tracks copied by Muspice are deleted. Files you put on the device yourself are not deleted.',
    noSource: 'Select what to sync',

    syncDialog: {
      title: (name) => `Sync to "${name}"`,
      planning: 'Checking the differences...',
      planFailed: (message) => `Could not check the differences: ${message}`,
      copy: 'Copy',
      delete: 'Delete',
      rename: 'Rename',
      unchanged: 'Unchanged',
      playlists: 'Playlists',
      countAndSize: (count, size) => `${plural(count, 'track', 'tracks')} (${size})`,
      freeSpace: 'Free space',
      upToDate: 'The tracks on the device are up to date',
      missingSources: (count) =>
        `The files of ${plural(count, 'track', 'tracks')} were not found (they cannot be copied; tracks already copied stay on the device)`,
      notEnoughSpace: (size) => `Not enough free space on the device (${size} more needed)`,
      start: 'Start Sync',
      preparing: 'Preparing...',
      copying: (current, total) => `Copying... (${current}/${total})`,
      finishing: 'Finishing...',
      stop: 'Stop',
      stopping: 'Stopping...',
      failed: (message) => `Sync failed: ${message}`,
      completed: 'Sync completed',
      cancelled: 'Sync stopped',
      cancelledHint:
        'The tracks already copied stay on the device. Sync again to continue from where it stopped.',
      copied: 'Copied:',
      deleted: 'Deleted:',
      renamed: 'Renamed:',
      playlistsWritten: 'Playlists:',
      errors: 'Errors:',
      errorDetails: 'Error details:'
    }
  }
};
