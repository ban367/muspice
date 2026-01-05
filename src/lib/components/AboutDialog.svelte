<script lang="ts">
  import { isAboutDialogOpen } from '$lib/stores/ui';

  function closeDialog() {
    isAboutDialogOpen.set(false);
  }
</script>

{#if $isAboutDialogOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="custom-modal-backdrop" onclick={closeDialog}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="about-content" onclick={(e) => e.stopPropagation()}>
      <div class="about-body">
        <!-- ロゴ -->
        <div class="about-logo">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="logo-icon"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M9 18V5l12-2v13" />
            <circle cx="6" cy="18" r="3" />
            <circle cx="18" cy="16" r="3" />
          </svg>
        </div>

        <!-- アプリ情報 -->
        <h2 class="about-title">Muspice</h2>
        <p class="about-version">バージョン 0.1.0</p>

        <p class="about-description">
          モダンな音楽プレイヤー<br />
          Built with Tauri + Svelte
        </p>

        <div class="about-links">
          <a
            href="https://github.com/ban367/muspice"
            target="_blank"
            rel="noopener noreferrer"
            class="about-link"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              class="link-icon"
              viewBox="0 0 24 24"
              fill="currentColor"
            >
              <path
                d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"
              />
            </svg>
            GitHub
          </a>
        </div>

        <button class="btn-primary about-close-btn" onclick={closeDialog}>閉じる</button>
      </div>
    </div>
  </div>
{/if}

<style>
  @reference "../../app.css";

  .about-content {
    @apply bg-base-300 rounded-xl w-full max-w-sm overflow-hidden shadow-xl;
  }

  .about-body {
    @apply p-8 text-center;
  }

  .about-logo {
    @apply w-20 h-20 mx-auto mb-4 rounded-2xl bg-primary/20 flex items-center justify-center;
  }

  .logo-icon {
    @apply w-12 h-12 text-primary;
  }

  .about-title {
    @apply text-2xl font-bold mb-2 m-0;
  }

  .about-version {
    @apply text-text-secondary mb-4 m-0;
  }

  .about-description {
    @apply text-sm text-text-muted mb-6 m-0;
  }

  .about-links {
    @apply mb-6;
  }

  .about-link {
    @apply inline-flex items-center gap-2 text-sm text-primary no-underline transition-colors;
  }

  .about-link:hover {
    @apply text-primary-focus;
  }

  .link-icon {
    @apply w-4 h-4;
  }

  .about-close-btn {
    @apply w-full;
  }
</style>
