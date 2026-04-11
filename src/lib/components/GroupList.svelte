<script lang="ts">
  import type { ChannelGroup } from '$lib/tauri';

  let { groups }: { groups: ChannelGroup[] } = $props();

  let expandedGroups = $state<Set<string>>(new Set());

  function toggleGroup(name: string) {
    const next = new Set(expandedGroups);
    if (next.has(name)) {
      next.delete(name);
    } else {
      next.add(name);
    }
    expandedGroups = next;
  }

  let totalChannels = $derived(groups.reduce((sum, g) => sum + g.count, 0));
</script>

<div class="group-list">
  <div class="group-header">
    <span class="group-total">{groups.length} groups &middot; {totalChannels} channels</span>
  </div>
  {#each groups as group (group.name)}
    <button class="group-row" onclick={() => toggleGroup(group.name)}>
      <span class="group-arrow" class:expanded={expandedGroups.has(group.name)}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 18 15 12 9 6"/></svg>
      </span>
      <span class="group-name">{group.name || 'Uncategorized'}</span>
      <span class="group-count">{group.count}</span>
    </button>
  {/each}
</div>

<style>
  .group-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .group-header {
    padding: 8px 0;
    margin-bottom: 4px;
  }

  .group-total {
    font-size: 13px;
    color: var(--color-text-muted);
    font-weight: 500;
  }

  .group-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    border-radius: 8px;
    background: transparent;
    color: var(--color-text);
    font-size: 13px;
    transition: background var(--transition-fast);
  }

  .group-row:hover {
    background: var(--color-hover);
  }

  .group-arrow {
    width: 16px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    transition: transform var(--transition-fast);
    flex-shrink: 0;
  }

  .group-arrow.expanded {
    transform: rotate(90deg);
  }

  .group-arrow :global(svg) {
    width: 14px;
    height: 14px;
  }

  .group-name {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .group-count {
    font-size: 12px;
    color: var(--color-text-muted);
    background: var(--color-surface);
    padding: 2px 8px;
    border-radius: 10px;
    flex-shrink: 0;
  }
</style>
