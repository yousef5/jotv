<script lang="ts">
  import { onMount } from 'svelte';
  import StatCard from '$lib/components/StatCard.svelte';
  import ChannelCard from '$lib/components/ChannelCard.svelte';
  import WelcomeScreen from '$lib/components/WelcomeScreen.svelte';
  import { getDashboardStats } from '$lib/tauri';
  import type { DashboardStats } from '$lib/tauri';
  import { playlists, loadPlaylists } from '$lib/stores/playlists';
  import { recentlyWatched, loadRecentlyWatched } from '$lib/stores/history';
  import { recommendations, loadRecommendations } from '$lib/stores/recommendations';

  let stats: DashboardStats | null = $state(null);
  let loaded = $state(false);

  onMount(async () => {
    await loadPlaylists();
    loaded = true;
    try {
      stats = await getDashboardStats();
    } catch (e) {
      console.error('Failed to load stats:', e);
    }
    loadRecentlyWatched(10);
    loadRecommendations(12);
  });
</script>

{#if loaded && $playlists.length === 0}
  <WelcomeScreen />
{:else}
  <div class="dashboard fade-in">
    <h1 class="page-title">Dashboard</h1>

    {#if stats}
      <div class="stats-grid">
        <StatCard label="Channels" value={stats.total_channels} color="var(--color-accent)" icon="channels" />
        <StatCard label="Favorites" value={stats.total_favorites} color="var(--color-accent-green)" icon="favorites" />
        <StatCard label="Playlists" value={stats.total_playlists} color="var(--color-accent-purple)" icon="playlists" />
        <StatCard label="Downloads" value={stats.total_downloads} color="var(--color-accent-yellow)" icon="downloads" />
      </div>
    {/if}

    {#if $recentlyWatched.length > 0}
      <section class="section">
        <h2 class="section-title">Continue Watching</h2>
        <div class="channel-grid">
          {#each $recentlyWatched as channel (channel.id)}
            <ChannelCard {channel} />
          {/each}
        </div>
      </section>
    {/if}

    {#if $recommendations.length > 0}
      <section class="section">
        <h2 class="section-title">Recommended For You</h2>
        <div class="channel-grid">
          {#each $recommendations as channel (channel.id)}
            <ChannelCard {channel} />
          {/each}
        </div>
      </section>
    {/if}
  </div>
{/if}

<style>
  .dashboard {
    display: flex;
    flex-direction: column;
    gap: 28px;
    padding-bottom: 40px;
  }

  .page-title {
    font-size: 24px;
    font-weight: 700;
    color: var(--color-text);
  }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 16px;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .section-title {
    font-size: 18px;
    font-weight: 600;
    color: var(--color-text);
  }

  .channel-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 16px;
  }
</style>
