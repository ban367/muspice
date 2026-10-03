<!--
  @component SettingsSync
  保存済みの設定（アクセントカラー）をメインウィンドウに反映する。表示は持たない。
  設定ウィンドウで保存されると、Rustが送る`SettingsChanged`イベントでキャッシュを更新する。
-->
<script lang="ts">
  import { useQueryClient } from '@tanstack/svelte-query';
  import { events } from '$lib/bindings';
  import { queryKeys } from '$lib/queries/keys';
  import { useSettingsQuery } from '$lib/queries/settings';
  import { applyAccentColor } from '$lib/utils/theme';

  const queryClient = useQueryClient();
  const settingsQuery = useSettingsQuery();

  $effect(() => {
    if (settingsQuery.data) {
      applyAccentColor(settingsQuery.data.accentColor);
    }
  });

  $effect(() => {
    const unlisten = events.settingsChanged.listen((event) => {
      queryClient.setQueryData(queryKeys.settings, event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>
