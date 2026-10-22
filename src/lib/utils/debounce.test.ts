import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createDebounce, createSearchDebounce } from './debounce.js';

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('createDebounce', () => {
  it('続けて呼んだ間は待ち、最後の呼び出しの引数で1回だけ呼ぶ', () => {
    const callback = vi.fn();
    const { debounced } = createDebounce(callback, 300);

    debounced('a');
    vi.advanceTimersByTime(200);
    debounced('ab');
    vi.advanceTimersByTime(299);
    expect(callback).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(callback).toHaveBeenCalledExactlyOnceWith('ab');
  });

  it('取り消すと、待っている呼び出しを呼ばない（取り消した後も、また使える）', () => {
    const callback = vi.fn();
    const { debounced, cancel } = createDebounce(callback);

    debounced(1);
    cancel();
    vi.advanceTimersByTime(1000);
    expect(callback).not.toHaveBeenCalled();

    debounced(2);
    vi.advanceTimersByTime(300);
    expect(callback).toHaveBeenCalledExactlyOnceWith(2);
    // 待っている呼び出しがない時に取り消しても、何も起きない
    cancel();
  });
});

describe('createSearchDebounce', () => {
  it('既定の300msの後に、最後の入力で検索する', () => {
    const onSearch = vi.fn();
    const { handleInput, cancel } = createSearchDebounce(onSearch);

    handleInput('d');
    handleInput("don't");
    vi.advanceTimersByTime(300);
    expect(onSearch).toHaveBeenCalledExactlyOnceWith("don't");

    handleInput('x');
    cancel();
    vi.advanceTimersByTime(300);
    expect(onSearch).toHaveBeenCalledOnce();
  });
});
