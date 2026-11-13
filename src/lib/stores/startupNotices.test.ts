import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('#lib/bindings.js', () => ({
  commands: {
    takeStartupNotices: vi.fn()
  }
}));
vi.mock('#lib/utils/dialog.svelte.js', () => ({
  showNotice: vi.fn(async () => {})
}));

import { commands } from '#lib/bindings.js';
import { showNotice } from '#lib/utils/dialog.svelte.js';
import { showStartupNotices, startupNoticeMessage } from './startupNotices.js';

beforeEach(() => {
  vi.clearAllMocks();
});

describe('startupNoticeMessage', () => {
  it('開けなかったDBの場所と、バックアップからの復元の案内を入れる', () => {
    const message = startupNoticeMessage({
      kind: 'databaseQuarantined',
      quarantinedPath: '/data/muspice-unreadable-20261113-093000.db',
      hasBackups: true
    });

    expect(message).toContain('空のライブラリで起動しました');
    expect(message).toContain('/data/muspice-unreadable-20261113-093000.db');
    expect(message).toContain('バックアップから復元...');
  });

  it('バックアップがなければ、復元ではなく読み込み直しを案内する', () => {
    const message = startupNoticeMessage({
      kind: 'databaseQuarantined',
      quarantinedPath: '/data/old.db',
      hasBackups: false
    });

    expect(message).toContain('バックアップは見つかりませんでした');
    expect(message).not.toContain('バックアップから復元...');
  });

  it('復元の失敗は、理由と、次の起動でもう一度試すことを知らせる', () => {
    const message = startupNoticeMessage({
      kind: 'restoreFailed',
      detail: 'DBを置き換えられません'
    });

    expect(message).toContain('復元する前のまま');
    expect(message).toContain('詳細: DBを置き換えられません');
  });
});

describe('showStartupNotices', () => {
  it('預かってある知らせを、順にダイアログで知らせる', async () => {
    vi.mocked(commands.takeStartupNotices).mockResolvedValue([
      { kind: 'restoreFailed', detail: 'x' },
      { kind: 'databaseQuarantined', quarantinedPath: '/data/old.db', hasBackups: true }
    ]);

    await showStartupNotices();

    expect(showNotice).toHaveBeenCalledTimes(2);
    expect(vi.mocked(showNotice).mock.calls[0][0]).toContain('復元に失敗しました');
    expect(vi.mocked(showNotice).mock.calls[1][0]).toContain('/data/old.db');
  });

  it('知らせがなければ、ダイアログを出さない', async () => {
    vi.mocked(commands.takeStartupNotices).mockResolvedValue([]);

    await showStartupNotices();

    expect(showNotice).not.toHaveBeenCalled();
  });
});
