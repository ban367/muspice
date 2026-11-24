import { QueryClient } from '@tanstack/svelte-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RefreshMetadataResult } from '#lib/bindings.js';

vi.mock('#lib/bindings.js', () => ({
  commands: {
    refreshLibraryMetadata: vi.fn()
  }
}));
vi.mock('#lib/utils/dialog.svelte.js', () => ({
  confirmDestructive: vi.fn(async () => true)
}));
vi.mock('#lib/stores/error.svelte.js', () => ({
  handleError: vi.fn(),
  showSuccess: vi.fn(),
  showWarning: vi.fn()
}));

import { commands } from '#lib/bindings.js';
import { handleError, showSuccess, showWarning } from '#lib/stores/error.svelte.js';
import { confirmDestructive } from '#lib/utils/dialog.svelte.js';
import { m } from '#lib/i18n/i18n.svelte.js';
import { reloadLibrary } from './libraryReload.js';

function result(skippedCount = 0, errorCount = 0): RefreshMetadataResult {
  return { updatedCount: 10, skippedCount, errorCount, errors: [] };
}

let queryClient: QueryClient;

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(confirmDestructive).mockResolvedValue(true);
  vi.mocked(commands.refreshLibraryMetadata).mockResolvedValue(result());
  queryClient = new QueryClient();
});

describe('reloadLibrary', () => {
  it('確かめてから読み込み直し、曲の一覧のキャッシュを取り直させて、終わったことを知らせる', async () => {
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');

    await reloadLibrary(queryClient);

    expect(confirmDestructive).toHaveBeenCalledWith(m.libraryReload.confirm, m.libraryReload.run);
    expect(commands.refreshLibraryMetadata).toHaveBeenCalledTimes(1);
    expect(invalidate).toHaveBeenCalled();
    expect(showSuccess).toHaveBeenNthCalledWith(1, m.libraryReload.running);
    expect(showSuccess).toHaveBeenLastCalledWith('ライブラリを再読み込みしました');
    expect(showWarning).not.toHaveBeenCalled();
  });

  it('確かめるダイアログでやめたら、読み込まない', async () => {
    vi.mocked(confirmDestructive).mockResolvedValue(false);

    await reloadLibrary(queryClient);

    expect(commands.refreshLibraryMetadata).not.toHaveBeenCalled();
    expect(showSuccess).not.toHaveBeenCalled();

    // やめた後は、もう一度選べる
    vi.mocked(confirmDestructive).mockResolvedValue(true);
    await reloadLibrary(queryClient);
    expect(commands.refreshLibraryMetadata).toHaveBeenCalledTimes(1);
  });

  it('見つからない曲だけなら数を添えて知らせ、読み込めなかった曲があれば警告で知らせる', async () => {
    vi.mocked(commands.refreshLibraryMetadata).mockResolvedValue(result(2, 0));
    await reloadLibrary(queryClient);
    expect(showSuccess).toHaveBeenLastCalledWith(
      'ライブラリを再読み込みしました（見つからない曲: 2曲）'
    );

    vi.mocked(commands.refreshLibraryMetadata).mockResolvedValue(result(2, 1));
    await reloadLibrary(queryClient);
    expect(showWarning).toHaveBeenCalledWith(
      'ライブラリを再読み込みしました（見つからない曲: 2曲、読み込めなかった曲: 1曲）'
    );
  });

  it('失敗したらトーストで知らせ、キャッシュは取り直させない', async () => {
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
    const error = { code: 'IO_ERROR', message: '読み込めません' };
    vi.mocked(commands.refreshLibraryMetadata).mockRejectedValue(error);

    await reloadLibrary(queryClient);

    expect(handleError).toHaveBeenCalledWith(error, m.operations.reloadLibrary);
    expect(invalidate).not.toHaveBeenCalled();
    expect(showSuccess).toHaveBeenCalledTimes(1);

    // 失敗した後は、もう一度選べる
    vi.mocked(commands.refreshLibraryMetadata).mockResolvedValue(result());
    await reloadLibrary(queryClient);
    expect(commands.refreshLibraryMetadata).toHaveBeenCalledTimes(2);
  });

  it('確かめている間・読み込んでいる間に選ばれても、始め直さない', async () => {
    let answer: (confirmed: boolean) => void = () => {};
    vi.mocked(confirmDestructive).mockReturnValue(
      new Promise((resolve) => {
        answer = resolve;
      })
    );
    let finish: (value: RefreshMetadataResult) => void = () => {};
    vi.mocked(commands.refreshLibraryMetadata).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      })
    );

    const first = reloadLibrary(queryClient);
    // 確かめている間は、何もしない
    await reloadLibrary(queryClient);
    expect(confirmDestructive).toHaveBeenCalledTimes(1);

    answer(true);
    await vi.waitFor(() => expect(commands.refreshLibraryMetadata).toHaveBeenCalledTimes(1));
    // 読み込んでいる間は、読み込み中であることだけを知らせる
    await reloadLibrary(queryClient);
    expect(confirmDestructive).toHaveBeenCalledTimes(1);
    expect(showSuccess).toHaveBeenLastCalledWith(m.libraryReload.running);

    finish(result());
    await first;
    expect(commands.refreshLibraryMetadata).toHaveBeenCalledTimes(1);
    expect(showSuccess).toHaveBeenLastCalledWith('ライブラリを再読み込みしました');
  });
});
