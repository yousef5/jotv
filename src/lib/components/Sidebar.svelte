<script lang="ts">
  import { page } from '$app/stores';

  const navItems = [
    { path: '/', label: 'Dashboard', icon: 'home' },
    { path: '/playlists', label: 'Playlists', icon: 'playlist' },
    { path: '/favorites', label: 'Favorites', icon: 'heart' },
    { path: '/downloads', label: 'Downloads', icon: 'download' },
    { path: '/social', label: 'Social Downloader', icon: 'social' },
  ];

  const bottomItems = [
    { path: '/settings', label: 'Settings', icon: 'settings' },
  ];
</script>

<nav class="sidebar">
  <div class="sidebar-top">
    <a href="/" class="logo" title="JoTV">
      <span class="logo-j">J</span><span class="logo-tv">TV</span>
    </a>
    {#each navItems as item}
      <a
        href={item.path}
        class="nav-item"
        class:active={$page.url.pathname === item.path}
        title={item.label}
      >
        <span class="nav-icon">
          {#if item.icon === 'home'}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 12l9-9 9 9"/><path d="M5 10v10a1 1 0 001 1h3a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1h3a1 1 0 001-1V10"/></svg>
          {:else if item.icon === 'playlist'}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 6h16M4 10h16M4 14h10"/><path d="M18 14v6M15 17h6"/></svg>
          {:else if item.icon === 'heart'}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/></svg>
          {:else if item.icon === 'download'}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
          {:else if item.icon === 'social'}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/></svg>
          {/if}
        </span>
      </a>
    {/each}
  </div>
  <div class="sidebar-bottom">
    {#each bottomItems as item}
      <a
        href={item.path}
        class="nav-item"
        class:active={$page.url.pathname === item.path}
        title={item.label}
      >
        <span class="nav-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 01-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/></svg>
        </span>
      </a>
    {/each}
  </div>
</nav>

<style>
  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    width: 60px;
    height: 100vh;
    background: var(--color-sidebar);
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    z-index: 100;
    border-right: 1px solid var(--color-border);
  }

  .sidebar-top, .sidebar-bottom {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .sidebar-top {
    padding-top: 8px;
  }

  .sidebar-bottom {
    padding-bottom: 16px;
  }

  .logo {
    width: 42px;
    height: 42px;
    display: flex;
    align-items: baseline;
    justify-content: center;
    padding-top: 8px;
    background: var(--color-accent);
    border-radius: 10px;
    margin-bottom: 12px;
    text-decoration: none;
    transition: all var(--transition-fast);
    gap: 0;
  }

  .logo:hover {
    transform: scale(1.08);
    box-shadow: 0 4px 12px rgba(233, 69, 96, 0.4);
  }

  .logo-j {
    font-size: 20px;
    font-weight: 800;
    color: #fff;
    line-height: 1;
  }

  .logo-tv {
    font-size: 10px;
    font-weight: 700;
    color: var(--color-accent-green);
    line-height: 1;
    margin-top: 2px;
  }

  .nav-item {
    width: 42px;
    height: 42px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 10px;
    color: var(--color-text-muted);
    text-decoration: none;
    transition: all var(--transition-fast);
    position: relative;
  }

  .nav-item:hover {
    color: var(--color-text);
    background: var(--color-hover);
  }

  .nav-item.active {
    color: var(--color-accent);
    background: rgba(233, 69, 96, 0.12);
  }

  .nav-item.active::before {
    content: '';
    position: absolute;
    left: -9px;
    top: 50%;
    transform: translateY(-50%);
    width: 3px;
    height: 20px;
    background: var(--color-accent);
    border-radius: 0 3px 3px 0;
  }

  .nav-icon {
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .nav-icon :global(svg) {
    width: 20px;
    height: 20px;
  }
</style>
