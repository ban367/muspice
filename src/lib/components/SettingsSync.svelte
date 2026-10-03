<!--
  @component SettingsSync
  保存済みの設定（言語・テーマ・アクセントカラー）をメインウィンドウに反映する。表示は持たない。
  設定ウィンドウで保存されると、Rustが送る`SettingsChanged`イベントでキャッシュを更新する。
-->
<script lang="ts">
  import { useQueryClient } from '@tanstack/svelte-query';
  import { events } from '#lib/bindings.js';
  import { queryKeys } from '#lib/queries/keys.js';
  import { useSettingsQuery } from '#lib/queries/settings.js';
  import { applyAccentColor, applyTheme } from '#lib/utils/theme.js';
  import { applyLanguage } from '#lib/i18n/i18n.svelte.js';

  const queryClient = useQueryClient();
  const settingsQuery = useSettingsQuery();

  $effect(() => {
    if (settingsQuery.data) {
      applyAccentColor(settingsQuery.data.accentColor);
    }
  });

  // 言語（変わると、画面の文言が作り直さずに切り替わる）
  $effect(() => {
    const language = settingsQuery.data?.language;
    if (language) applyLanguage(language);
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
