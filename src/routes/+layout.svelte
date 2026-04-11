<script lang="ts">
  import { page } from '$app/stores';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import '../app.css';

  let { children } = $props();

  let isPlayerRoute = $derived($page.url.pathname === '/player');
</script>

<svelte:head>
  <title>JoTV</title>
</svelte:head>

{#if !isPlayerRoute}
  <Sidebar />
{/if}

<main class:has-sidebar={!isPlayerRoute} class:cinema-mode={isPlayerRoute}>
  {@render children()}
</main>

<style>
  main.has-sidebar {
    margin-left: 60px;
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
