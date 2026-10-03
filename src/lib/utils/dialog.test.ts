import { describe, expect, it, vi } from 'vitest';
import { promptText, textPrompt } from './dialog.svelte.js';

vi.mock('@tauri-apps/plugin-dialog', () => ({ confirm: vi.fn() }));

describe('promptText', () => {
  it('表示中の要求を公開し、確定した値で解決して閉じる', async () => {
    const result = promptText({ title: '新規プレイリスト', label: 'プレイリスト名' });

    const request = textPrompt.request;
    expect(request?.title).toBe('新規プレイリスト');

    request?.resolve('作業用BGM');
    await expect(result).resolves.toBe('作業用BGM');
    expect(textPrompt.request).toBeNull();
  });

  it('キャンセルはnullで解決する', async () => {
    const result = promptText({ title: 't', label: 'l' });
    textPrompt.request?.resolve(null);
    await expect(result).resolves.toBeNull();
  });

  it('表示中に別の入力を求めると、前の要求はキャンセルになり新しい要求が残る', async () => {
    const first = promptText({ title: '1つ目', label: 'l' });
    const firstRequest = textPrompt.request;
    const second = promptText({ title: '2つ目', label: 'l' });

    await expect(first).resolves.toBeNull();
    expect(textPrompt.request?.title).toBe('2つ目');

    // 置き換えられた要求を後から解決しても、表示中の要求は閉じない
    firstRequest?.resolve('遅れた回答');
    expect(textPrompt.request?.title).toBe('2つ目');

    textPrompt.request?.resolve('OK');
    await expect(second).resolves.toBe('OK');
  });
});
