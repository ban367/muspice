import { describe, expect, it } from 'vitest';
import { describeRelocationBlocker } from './relocation';

describe('describeRelocationBlocker', () => {
  it('付け替えられない理由ごとに、選んだ場所を含む文を返す', () => {
    expect(describeRelocationBlocker({ kind: 'overlapsCurrent', path: '/Music/Sub' })).toContain(
      '「/Music/Sub」は今の場所と重なる'
    );
    expect(
      describeRelocationBlocker({
        kind: 'overlapsLibraryFolder',
        path: '/Other/In',
        libraryFolderPath: '/Other'
      })
    ).toContain('ライブラリフォルダ「/Other」と重なる');
    expect(
      describeRelocationBlocker({ kind: 'overlapsDevice', path: '/SD', deviceName: 'SDカード' })
    ).toContain('転送先デバイス「SDカード」');
    expect(
      describeRelocationBlocker({ kind: 'conflicts', path: '/New', conflictCount: 3 })
    ).toContain('3曲');
  });
});
