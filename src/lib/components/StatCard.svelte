<script lang="ts">
  let { label, value, color = 'var(--color-accent)', icon = '' }: {
    label: string;
    value: string | number;
    color?: string;
    icon?: string;
  } = $props();
</script>

<div class="stat-card" style="--stat-color: {color}">
  <div class="stat-icon">
    {#if icon === 'channels'}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>
    {:else if icon === 'favorites'}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
    {:else if icon === 'playlists'}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 6h16M4 10h16M4 14h10"/><path d="M18 14v6M15 17h6"/></svg>
    {:else if icon === 'downloads'}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
    {:else}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
    {/if}
  </div>
  <div class="stat-info">
    <span class="stat-value">{value}</span>
    <span class="stat-label">{label}</span>
  </div>
</div>

<style>
  .stat-card {
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-card);
    padding: 20px;
    display: flex;
    align-items: center;
    gap: 16px;
    transition: transform var(--transition-fast), box-shadow var(--transition-fast);
  }

  .stat-card:hover {
    transform: translateY(-2px);
    box-shadow: 0 8px 25px rgba(0, 0, 0, 0.3);
  }

  .stat-icon {
    width: 48px;
    height: 48px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--stat-color) 15%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--stat-color);
    flex-shrink: 0;
  }

  .stat-icon :global(svg) {
    width: 24px;
    height: 24px;
  }

  .stat-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stat-value {
    font-size: 28px;
    font-weight: 700;
    line-height: 1;
    color: var(--color-text);
  }

  .stat-label {
    font-size: 13px;
    color: var(--color-text-muted);
  }
</style>
