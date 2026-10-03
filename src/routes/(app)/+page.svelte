<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import type { Path } from '$app/types';
  import { useSettingsQuery } from '#lib/queries/settings.js';
  import { getLastPage } from '#lib/stores/ui.svelte.js';

  const settingsQuery = useSettingsQuery();
  let redirected = false;

  // 起動時の画面: 設定に応じて前回開いていた画面か曲一覧を開く
  // （設定を読めなかった場合や、記録がない場合は曲一覧）
  $effect(() => {
    if (redirected || settingsQuery.isPending) return;
    redirected = true;

    const lastPage = settingsQuery.data?.startupPage === 'lastOpened' ? getLastPage() : null;
    const songsPage = resolve('library/songs');
    // 記録はpathname（先頭が'/'）のため、'/'を外してresolveに渡す
    // （resolveは先頭が'/'の文字列をルートIDとして扱い、'(...)'を含むジャンル名をルートグループとして消してしまう）
    const target = lastPage ? resolve(lastPage.slice(1) as Path) : songsPage;
    // 記録した画面が現在のルートにない場合、gotoは拒否されるため曲一覧を開く
    goto(target, { replace: true }).catch(() => goto(songsPage, { replace: true }));
  });
</script>

<div class="redirect-page">
  <p>読み込み中...</p>
</div>

<style>
  .redirect-page {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--color-text-muted);
  }
</style>
