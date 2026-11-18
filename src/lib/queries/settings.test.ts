import { QueryClient } from '@tanstack/svelte-query';
import { describe, expect, it } from 'vitest';
import type { Settings } from '#lib/types/models.js';
import { queryKeys } from './keys.js';
import { receiveSavedSettings } from './settings.js';

/** テストで見分けるだけの設定（キャッシュに入れる値として使う） */
function settingsWith(theme: Settings['theme']): Settings {
  return { theme } as Settings;
}

describe('receiveSavedSettings', () => {
  it('ほかのウィンドウで保存された設定を、キャッシュへ入れる', async () => {
    const client = new QueryClient();
    client.setQueryData(queryKeys.settings, settingsWith('dark'));

    await receiveSavedSettings(client, settingsWith('light'));

    expect(client.getQueryData<Settings>(queryKeys.settings)?.theme).toBe('light');
  });

  it('取得中の設定は取り消し、後から終わった古い値で上書きしない', async () => {
    const client = new QueryClient();
    let finishFetch: (settings: Settings) => void = () => {};
    // 通知より前の値を読んでいる、最初の取得
    const fetching = client
      .fetchQuery({
        queryKey: queryKeys.settings,
        queryFn: () =>
          new Promise<Settings>((resolve) => {
            finishFetch = resolve;
          })
      })
      .catch(() => null);

    await receiveSavedSettings(client, settingsWith('light'));
    finishFetch(settingsWith('dark'));
    await fetching;

    expect(client.getQueryData<Settings>(queryKeys.settings)?.theme).toBe('light');
  });
});
