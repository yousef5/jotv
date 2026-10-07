<script lang="ts">
  import { page } from '$app/stores';
  import { nowPlaying } from '$lib/stores/live';
  import { searchPalette, openSearch } from '$lib/stores/search';

  interface NavItem {
    href: string;
    label: string;
    icon: string;
    match: (path: string, params: URLSearchParams) => boolean;
  }

  const navItems: NavItem[] = [
    { href: '/', label: 'Home', icon: 'home', match: (p) => p === '/' },
    { href: '/live', label: 'Live TV', icon: 'live', match: (p) => p === '/live' },
    { href: '/browse?type=vod', label: 'Movies', icon: 'movies', match: (p, q) => p === '/browse' && q.get('type') !== 'series' },
    { href: '/browse?type=series', label: 'Series', icon: 'series', match: (p, q) => p === '/browse' && q.get('type') === 'series' },
    { href: '/favorites', label: 'My list', icon: 'heart', match: (p) => p === '/favorites' },
    { href: '/downloads', label: 'Downloads', icon: 'download', match: (p) => p === '/downloads' },
    { href: '/social', label: 'Video library', icon: 'social', match: (p) => p === '/social' || p === '/reel' },
  ];

  const bottomItems: NavItem[] = [
    { href: '/playlists', label: 'Playlists', icon: 'playlists', match: (p) => p === '/playlists' },
    { href: '/settings', label: 'Settings', icon: 'settings', match: (p) => p === '/settings' },
  ];
</script>

{#snippet glyph(name: string)}
  {#if name === 'home'}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 11l9-8 9 8"/><path d="M5 9.5V20a1 1 0 001 1h4v-6h4v6h4a1 1 0 001-1V9.5"/></svg>
  {:else if name === 'live'}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="6" width="19" height="13" rx="2.5"/><polyline points="8 2.5 12 6 16 2.5"/></svg>
  {:else if name === 'movies'}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2.5"/><path d="M7 3v18M17 3v18M3 8h4M3 16h4M17 8h4M17 16h4M3 12h18"/></svg>
  {:else if name === 'series'}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="7" width="18" height="14" rx="2.5"/><path d="M6 3.5h12"/><path d="M10.5 11v6l4.5-3z" fill="currentColor"/></svg>
  {:else if name === 'heart'}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M12 20.5s-7.5-4.6-9.6-9.4C.9 7.6 3.2 4 6.9 4c2.1 0 3.6 1.1 5.1 3 1.5-1.9 3-3 5.1-3 3.7 0 6 3.6 4.5 7.1C19.5 15.9 12 20.5 12 20.5z"/></svg>
  {:else if name === 'download'}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3.5v12"/><polyline points="7 10.5 12 15.5 17 10.5"/><path d="M4.5 20.5h15"/></svg>
  {:else if name === 'social'}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><line x1="8.6" y1="13.5" x2="15.4" y2="17.5"/><line x1="15.4" y1="6.5" x2="8.6" y2="10.5"/></svg>
  {:else if name === 'playlists'}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M4 6h16M4 11h16M4 16h9"/><path d="M18 14v6M15 17h6"/></svg>
  {:else if name === 'settings'}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 01-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/></svg>
  {/if}
{/snippet}

{#snippet link(item: NavItem)}
  {@const active = item.match($page.url.pathname, $page.url.searchParams)}
  <a href={item.href} class="nav-item" class:active aria-label={item.label} aria-current={active ? 'page' : undefined}>
    <span class="nav-icon">{@render glyph(item.icon)}</span>
    {#if item.icon === 'live' && $nowPlaying}<span class="on-air" title="Playing now"></span>{/if}
    <span class="nav-tip">{item.label}</span>
  </a>
{/snippet}

<nav class="sidebar">
  <div class="sidebar-top">
    <a href="/" class="logo" aria-label="JoTV home">
      <span class="logo-j">J</span><span class="logo-tv">TV</span>
    </a>
    <button class="nav-item" class:active={$searchPalette.open} onclick={() => openSearch()} aria-label="Search">
      <span class="nav-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><line x1="16.5" y1="16.5" x2="21" y2="21"/></svg>
      </span>
      <span class="nav-tip">Search · Ctrl K</span>
    </button>
    {#each navItems as item (item.href)}{@render link(item)}{/each}
  </div>
  <div class="sidebar-bottom">
    {#each bottomItems as item (item.href)}{@render link(item)}{/each}
  </div>
</nav>

<style>
  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    width: 72px;
    height: 100vh;
    background: var(--color-sidebar);
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    z-index: 100;
    box-shadow: 1px 0 0 oklch(1 0 0 / 0.04);
  }

  .sidebar-top, .sidebar-bottom {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }

  .sidebar-top {
    padding-top: 18px;
  }

  .sidebar-bottom {
    padding-bottom: 18px;
  }

  .logo {
    display: flex;
    align-items: baseline;
    justify-content: center;
    margin-bottom: 22px;
    text-decoration: none;
    transition: transform 200ms var(--ease-out);
  }

  .logo:hover {
    transform: scale(1.08);
  }

  .logo-j {
    font-size: 30px;
    font-weight: 900;
    color: var(--color-accent);
    line-height: 1;
    letter-spacing: -0.04em;
  }

  .logo-tv {
    font-size: 11px;
    font-weight: 800;
    color: var(--color-text);
    line-height: 1;
    letter-spacing: 0.04em;
  }

  button.nav-item { padding: 0; background: none; }

  .nav-item {
    width: 44px;
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 12px;
    color: var(--color-text-muted);
    text-decoration: none;
    transition: color 200ms var(--ease-out), background 200ms var(--ease-out);
    position: relative;
  }

  .nav-item:hover {
    color: var(--color-text);
    background: oklch(1 0 0 / 0.06);
  }

  .nav-item.active {
    color: var(--color-text);
    background: oklch(1 0 0 / 0.1);
  }

  .nav-item.active::after {
    content: '';
    position: absolute;
    bottom: 5px;
    left: 50%;
    width: 4px;
    height: 4px;
    margin-left: -2px;
    border-radius: 50%;
    background: var(--color-accent);
  }

  .nav-item:focus-visible {
    outline: 2px solid var(--color-text);
    outline-offset: 2px;
  }

  .nav-tip {
    position: absolute;
    left: calc(100% + 14px);
    top: 50%;
    transform: translate(-4px, -50%);
    padding: 6px 10px;
    border-radius: 5px;
    background: var(--color-text);
    color: oklch(0.14 0.004 25);
    font-size: 0.8125rem;
    font-weight: 700;
    white-space: nowrap;
    opacity: 0;
    pointer-events: none;
    transition: opacity 120ms var(--ease-out), transform 120ms var(--ease-out);
    z-index: 200;
  }
  .nav-item:hover .nav-tip,
  .nav-item:focus-visible .nav-tip {
    opacity: 1;
    transform: translate(0, -50%);
  }

  .on-air {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-accent);
    box-shadow: 0 0 0 2px var(--color-sidebar);
    animation: onair 1.6s ease-in-out infinite;
  }
  @keyframes onair { 50% { opacity: 0.4; } }

  @media (prefers-reduced-motion: reduce) {
    .on-air { animation: none; }
  }

  .nav-icon {
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .nav-icon :global(svg) {
    width: 22px;
    height: 22px;
    stroke-width: 1.75;
  }
</style>
