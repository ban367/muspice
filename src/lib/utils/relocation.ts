/**
 * ライブラリフォルダの付け替え先を選んだ結果の扱いと通知文
 */
import { m } from '#lib/i18n/i18n.svelte.js';
import type { FolderRelocation } from '#lib/types/models.js';

/** 付け替えられる場所を選んだ結果（Rust側が場所を預かっている） */
export type ReadyRelocation = Extract<FolderRelocation, { kind: 'ready' }>;

/** 付け替えられない場所を選んだ結果 */
export type BlockedRelocation = Exclude<FolderRelocation, { kind: 'ready' }>;

/** 付け替えられない場所を選んだ理由の文 */
export function describeRelocationBlocker(relocation: BlockedRelocation): string {
  const messages = m.libraryFolders;
  switch (relocation.kind) {
    case 'overlapsCurrent':
      return messages.relocateOverlapsCurrent(relocation.path);
    case 'overlapsLibraryFolder':
      return messages.relocateOverlapsLibraryFolder(relocation.path, relocation.libraryFolderPath);
    case 'overlapsDevice':
      return messages.relocateOverlapsDevice(relocation.path, relocation.deviceName);
    case 'conflicts':
      return messages.relocateConflicts(relocation.path, relocation.conflictCount);
  }
}
