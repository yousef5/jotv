<script lang="ts">
  import ChannelCard from '$lib/components/ChannelCard.svelte';
  import { searchResults, searchChannelsStore } from '$lib/stores/channels';

  let query = $state('');
  let searchTimeout: ReturnType<typeof setTimeout> | null = null;

  function handleInput() {
    if (searchTimeout) clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => {
      searchChannelsStore(query);
    }, 300);
  }
</script>

<div class="search-page fade-in">
  <h1 class="page-title">Search</h1>

  <div class="search-bar">
    <svg class="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
    <input
      type="text"
      placeholder="Search channels..."
      bind:value={query}
      oninput={handleInput}
      class="search-input"
    />
    {#if query}
      <button class="clear-btn" onclick={() => { query = ''; searchResults.set([]); }} title="Clear search">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
      </button>
    {/if}
  </div>

  {#if $searchResults.length > 0}
    <div class="results-count">{$searchResults.length} result{$searchResults.length !== 1 ? 's' : ''}</div>
    <div class="results-grid">
      {#each $searchResults as channel (channel.id)}
        <ChannelCard {channel} />
      {/each}
    </div>
  {:else if query.length > 0}
    <div class="empty">
      <p>No channels found for "{query}"</p>
    </div>
  {:else}
    <div class="empty">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
      <p>Type to search across all channels</p>
    </div>
  {/if}
</div>

<style>
  .search-page {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding-bottom: 40px;
  }

  .page-title {
    font-size: 24px;
    font-weight: 700;
  }

  .search-bar {
    position: relative;
    display: flex;
    align-items: center;
  }

  .search-icon {
    position: absolute;
    left: 14px;
    width: 18px;
    height: 18px;
    color: var(--color-text-muted);
    pointer-events: none;
  }

  .search-input {
    width: 100%;
    padding: 12px 44px 12px 44px;
    font-size: 15px;
    border-radius: 12px;
    background: var(--color-card);
    border: 1px solid var(--color-border);
  }

  .search-input:focus {
    border-color: var(--color-accent);
  }

  .clear-btn {
    position: absolute;
    right: 10px;
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
  }

  .clear-btn:hover {
    color: var(--color-text);
    background: var(--color-hover);
  }

  .clear-btn :global(svg) {
    width: 16px;
    height: 16px;
  }

  .results-count {
    font-size: 13px;
    color: var(--color-text-muted);
  }

  .results-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 16px;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 80px 0;
    color: var(--color-text-muted);
  }

  .empty :global(svg) {
    width: 48px;
    height: 48px;
    opacity: 0.3;
  }

  .empty p {
    font-size: 14px;
  }
</style>
