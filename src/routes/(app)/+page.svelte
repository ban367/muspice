<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import type { Pathname } from '$app/types';
  import { useSettingsQuery } from '$lib/queries/settings';
  import { getLastPage } from '$lib/stores/ui';

  const settingsQuery = useSettingsQuery();
  let redirected = false;

  // 起動時の画面: 設定に応じて前回開いていた画面か曲一覧を開く
  // （設定を読めなかった場合や、記録がない場合は曲一覧）
  $effect(() => {
    if (redirected || settingsQuery.isPending) return;
    redirected = true;

    const lastPage = settingsQuery.data?.startupPage === 'lastOpened' ? getLastPage() : null;
    goto(lastPage ? resolve(lastPage as Pathname) : resolve('/library/songs'), {
      replaceState: true
    });
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
    color: #888;
  }
</style>
