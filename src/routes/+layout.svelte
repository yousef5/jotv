<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { afterNavigate } from '$app/navigation';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import VideoSurface from '$lib/components/VideoSurface.svelte';
  import SearchPalette from '$lib/components/SearchPalette.svelte';
  import FavoriteSaved from '$lib/components/FavoriteSaved.svelte';
  import { setupDownloadListeners } from '$lib/stores/downloads';
  import { syncOnLaunch } from '$lib/stores/sync';
  import '@fontsource-variable/inter';
  import '@fontsource/ibm-plex-sans-arabic/arabic-400.css';
  import '@fontsource/ibm-plex-sans-arabic/arabic-500.css';
  import '@fontsource/ibm-plex-sans-arabic/arabic-600.css';
  import '@fontsource/ibm-plex-sans-arabic/arabic-700.css';
  import '../app.css';

  let { children } = $props();

  // Full-window players: no sidebar, no page padding
  let isPlayerRoute = $derived(['/player', '/watch', '/reel'].includes($page.url.pathname));

  // Pages scroll inside <main>; start new pages at the top. Back/forward is
  // handled by each page's snapshot (see $lib/scroll).
  afterNavigate((nav) => {
    if (nav.type !== 'popstate') document.querySelector('main')?.scrollTo({ top: 0 });
  });

  onMount(() => {
    let cleanup: (() => void) | undefined;

    setupDownloadListeners().then((unlisten) => {
      cleanup = unlisten;
    });

    syncOnLaunch().catch((e) => console.error('Launch sync failed:', e));

    return () => {
      cleanup?.();
    };
  });
</script>

<svelte:head>
  <title>JoTV</title>
</svelte:head>

{#if !isPlayerRoute}
  <Sidebar />
{/if}

<VideoSurface />
<SearchPalette />
<FavoriteSaved />

<main class:has-sidebar={!isPlayerRoute} class:cinema-mode={isPlayerRoute}>
  {@render children()}
</main>

<style>
  main.has-sidebar {
    margin-left: 72px;
    height: 100vh;
    overflow-y: auto;
    padding: 24px;
  }

  main.cinema-mode {
    height: 100vh;
    width: 100vw;
    overflow: hidden;
    padding: 0;
  }
</style>
