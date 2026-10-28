<!--
  @component SidePanelMessage
  右サイドバーのパネルの本文に、中身の代わりに出す文（再生していない・選んでいない・曲がない、など）。
-->
<script lang="ts">
  interface Props {
    text: string;
  }

  let { text }: Props = $props();
</script>

<p class="panel-message">{text}</p>

<style>
  @reference "../../../app.css";

  .panel-message {
    @apply m-0 py-8 px-4 text-center text-sm text-text-dimmed;
  }
</style>
