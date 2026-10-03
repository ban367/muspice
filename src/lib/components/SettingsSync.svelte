<!--
  @component SettingsSync
  保存済みの設定（テーマ・アクセントカラー）をメインウィンドウに反映する。表示は持たない。
  設定ウィンドウで保存されると、Rustが送る`SettingsChanged`イベントでキャッシュを更新する。
-->
<script lang="ts">
  import { useQueryClient } from '@tanstack/svelte-query';
  import { events } from '#lib/bindings.js';
  import { queryKeys } from '#lib/queries/keys.js';
  import { useSettingsQuery } from '#lib/queries/settings.js';
  import { applyAccentColor, applyTheme } from '#lib/utils/theme.js';

  const queryClient = useQueryClient();
  const settingsQuery = useSettingsQuery();

  $effect(() => {
    if (settingsQuery.data) {
      applyAccentColor(settingsQuery.data.accentColor);
    }
  });

  // テーマ（「OSの設定に従う」の間はOSの配色の変化に追従し、テーマが変わったら追従をやめる）
  $effect(() => {
    const theme = settingsQuery.data?.theme;
    if (theme) return applyTheme(theme);
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
