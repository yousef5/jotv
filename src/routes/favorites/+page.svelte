<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { goto } from '$app/navigation';
  import {
    removeFavorite, addFavorite, getSetting, favoriteLists, favoriteListSave, favoriteListDelete, favoriteSetList,
  } from '$lib/tauri';
  import type { Channel, FavoriteChannel, FavoriteList } from '$lib/tauri';
  import Dropdown from '$lib/components/Dropdown.svelte';
  import { CATEGORY_COLORS } from '$lib/stores/reels';
  import { favorites, loadFavorites } from '$lib/stores/favorites';
  import { pendingOpen } from '$lib/stores/playlists';
  import { nowPlaying } from '$lib/stores/live';
  import { splitTitle } from '$lib/playback';

  type Tab = 'all' | 'live' | 'vod' | 'series';
  type Kind = Exclude<Tab, 'all'>;
  type Sort = 'added' | 'name';
  /** null = everything, 'none' = not in any category */
  type ListFilter = number | 'none' | null;

  const KIND_LABEL: Record<Kind, string> = { live: 'Channels', vod: 'Movies', series: 'Series' };
  const KIND_HINT: Record<Kind, string> = {
    live: 'Sort your channels into categories like Sports, News or Kids.',
    vod: 'Sort your movies into categories like Comedy, Watch this weekend or Classics.',
    series: 'Sort your series into categories like Watching now, Drama or Up next.',
  };

  let loaded = $state(false);
  let tab = $state<Tab>('all');
  let sort = $state<Sort>('added');
  let lists = $state<FavoriteList[]>([]);
  let listFilter = $state<ListFilter>(null);
  let query = $state('');
  let failed = $state<Set<number>>(new Set());
  /** Movies: watched fraction; series: "S1:E3" to continue */
  let progress = $state<Record<number, number>>({});
  let continueAt = $state<Record<number, string>>({});
  let undo = $state<FavoriteChannel | null>(null);
  let undoTimer: ReturnType<typeof setTimeout> | undefined;

  const nf = new Intl.NumberFormat('en-US');

  onMount(() => {
    load();
    return () => clearTimeout(undoTimer);
  });

  async function load() {
    await Promise.all([loadFavorites(), favoriteLists().then((l) => (lists = l)).catch(() => {})]);
    loaded = true;
    // Watch progress for posters (local settings, no network)
    for (const f of $favorites) {
      if (f.content_type === 'vod') {
        getSetting(`progress_vod_${f.channel_id}`).then((raw) => {
          const [pos, dur] = (raw ?? '').split('|').map(Number);
          if (pos > 0 && dur > 0) progress = { ...progress, [f.channel_id]: Math.min(1, pos / dur) };
        }).catch(() => {});
      } else if (f.content_type === 'series') {
        getSetting(`series_progress_${f.channel_id}`).then((raw) => {
          const [s, e] = (raw ?? '').split(':');
          if (s && e) continueAt = { ...continueAt, [f.channel_id]: `S${s}:E${e}` };
        }).catch(() => {});
      }
    }
  }

  let counts = $derived({
    all: $favorites.length,
    live: $favorites.filter((f) => f.content_type === 'live').length,
    vod: $favorites.filter((f) => f.content_type === 'vod').length,
    series: $favorites.filter((f) => f.content_type === 'series').length,
  });
  let kindLists = $derived(tab === 'all' ? [] : lists.filter((l) => l.kind === tab));
  /** A favorite's category, if it still exists and fits its type */
  function listOf(f: FavoriteChannel): FavoriteList | undefined {
    return f.list_id === null ? undefined : lists.find((l) => l.id === f.list_id && l.kind === f.content_type);
  }
  let listCounts = $derived.by(() => {
    const m = new Map<number | 'none', number>();
    for (const f of $favorites) {
      if (tab !== 'all' && f.content_type !== tab) continue;
      const k = listOf(f)?.id ?? 'none';
      m.set(k, (m.get(k) ?? 0) + 1);
    }
    return m;
  });

  function colorOf(l: FavoriteList): string {
    return l.color ?? CATEGORY_COLORS[l.id % CATEGORY_COLORS.length];
  }

  // A filter for another type makes no sense: reset when switching tabs
  $effect(() => {
    void tab;
    listFilter = null;
    draft = null;
  });

  let shown = $derived.by(() => {
    const term = query.trim().toLowerCase();
    let list = $favorites.filter(
      (f) =>
        (tab === 'all' || f.content_type === tab) &&
        (listFilter === null || (listFilter === 'none' ? !listOf(f) : listOf(f)?.id === listFilter)) &&
        (!term || f.channel_name.toLowerCase().includes(term) || f.group_name.toLowerCase().includes(term)),
    );
    list = [...list].sort((a, b) =>
      sort === 'name' ? a.channel_name.localeCompare(b.channel_name) : b.added_at.localeCompare(a.added_at),
    );
    return list;
  });

  /** Sections per type, each split into your categories (unsorted last) */
  let sections = $derived.by(() => {
    const kinds: Kind[] = tab === 'all' ? ['live', 'vod', 'series'] : [tab];
    return kinds
      .map((kind) => {
        const items = shown.filter((f) => f.content_type === kind);
        const own = lists.filter((l) => l.kind === kind);
        const grouped = listFilter === null && own.length > 0;
        const groups = grouped
          ? [
              ...own.map((l) => ({ list: l as FavoriteList | null, items: items.filter((f) => listOf(f)?.id === l.id) })),
              { list: null, items: items.filter((f) => !listOf(f)) },
            ].filter((g) => g.items.length)
          : [{ list: null, items }];
        // Only "Not sorted" left: no heading needed
        return { kind, count: items.length, grouped: grouped && groups.some((g) => g.list), groups };
      })
      .filter((s) => s.count);
  });

  // ── Categories: create, rename, delete ──
  let draft = $state<{ id: number | null; name: string } | null>(null);
  let draftEl = $state<HTMLInputElement | undefined>();
  let confirmDelete = $state<number | null>(null);

  async function startDraft(id: number | null, name = '') {
    confirmDelete = null;
    draft = { id, name };
    await tick();
    draftEl?.focus();
    draftEl?.select();
  }

  async function saveDraft() {
    const d = draft;
    if (!d || tab === 'all') return;
    draft = null;
    if (!d.name.trim()) return;
    try {
      const saved = await favoriteListSave(d.id, d.name, tab, d.id === null ? null : lists.find((l) => l.id === d.id)?.color ?? null);
      lists = d.id === null ? [...lists.filter((l) => l.id !== saved.id), saved] : lists.map((l) => (l.id === saved.id ? saved : l));
      if (d.id === null) listFilter = saved.id;
    } catch (e) {
      flash(String(e));
    }
  }

  async function deleteList(l: FavoriteList) {
    if (confirmDelete !== l.id) {
      confirmDelete = l.id;
      return;
    }
    confirmDelete = null;
    await favoriteListDelete(l.id).catch(() => {});
    lists = lists.filter((x) => x.id !== l.id);
    favorites.update((all) => all.map((f) => (f.list_id === l.id ? { ...f, list_id: null } : f)));
    if (listFilter === l.id) listFilter = null;
    flash(`Deleted “${l.name}”. Its items are still saved.`);
  }

  // ── Putting a favorite into a category ──
  let menuFor = $state<number | null>(null);
  let menuDraft = $state('');

  async function setList(f: FavoriteChannel, listId: number | null) {
    menuFor = null;
    const before = f.list_id;
    favorites.update((all) => all.map((x) => (x.channel_id === f.channel_id ? { ...x, list_id: listId } : x)));
    try {
      await favoriteSetList([f.channel_id], listId);
    } catch {
      favorites.update((all) => all.map((x) => (x.channel_id === f.channel_id ? { ...x, list_id: before } : x)));
    }
  }

  async function newListFor(f: FavoriteChannel) {
    const name = menuDraft.trim();
    if (!name) return;
    menuDraft = '';
    try {
      const saved = await favoriteListSave(null, name, f.content_type);
      lists = [...lists.filter((l) => l.id !== saved.id), saved];
      await setList(f, saved.id);
    } catch (e) {
      flash(String(e));
    }
  }

  function outside(e: PointerEvent) {
    if (menuFor !== null && !(e.target as HTMLElement).closest?.('.menu, .sort-btn')) menuFor = null;
  }

  let note = $state('');
  let noteTimer: ReturnType<typeof setTimeout> | undefined;
  function flash(msg: string) {
    note = msg;
    clearTimeout(noteTimer);
    noteTimer = setTimeout(() => (note = ''), 3500);
  }

  function asChannel(f: FavoriteChannel): Channel {
    return {
      id: f.channel_id,
      playlist_id: f.playlist_id,
      name: f.channel_name,
      group_name: f.group_name,
      stream_url: f.stream_url,
      logo_url: f.logo_url,
      epg_id: null,
      content_type: f.content_type,
      created_at: f.added_at,
    };
  }

  function open(f: FavoriteChannel) {
    if (f.content_type === 'live') {
      pendingOpen.set({ playlistId: f.playlist_id, contentType: 'live', channel: asChannel(f), action: 'play' });
      goto('/live');
    } else {
      goto(`/title?id=${f.channel_id}`);
    }
  }

  async function remove(f: FavoriteChannel) {
    favorites.update((list) => list.filter((x) => x.channel_id !== f.channel_id));
    await removeFavorite(f.channel_id).catch(() => loadFavorites());
    undo = f;
    clearTimeout(undoTimer);
    undoTimer = setTimeout(() => (undo = null), 6000);
  }

  async function restore() {
    const f = undo;
    if (!f) return;
    undo = null;
    clearTimeout(undoTimer);
    await addFavorite(f.channel_id, f.category).catch(() => {});
    if (f.list_id !== null) await favoriteSetList([f.channel_id], f.list_id).catch(() => {});
    await loadFavorites();
  }

  function addedAgo(ts: string): string {
    const t = Date.parse(ts.replace(' ', 'T') + 'Z');
    if (isNaN(t)) return '';
    const d = Math.floor((Date.now() - t) / 86400000);
    return d <= 0 ? 'Added today' : d === 1 ? 'Added yesterday' : d < 30 ? `Added ${d} days ago` : `Added ${new Date(t).toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}`;
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={(e) => e.key === 'Escape' && (menuFor = null)} />

{#snippet tile(f: FavoriteChannel)}
  {@const playing = $nowPlaying?.kind === 'live' && $nowPlaying.channel.id === f.channel_id}
  <button class="tile" class:playing onclick={() => open(f)} aria-label={`Watch ${f.channel_name}`}>
    <span class="tile-art">
      {#if f.logo_url && !failed.has(f.channel_id)}
        <img src={f.logo_url} alt="" loading="lazy" onerror={() => (failed = new Set(failed).add(f.channel_id))} />
      {:else}
        <span class="letter">{f.channel_name.trim().charAt(0).toUpperCase()}</span>
      {/if}
      <span class="badge live"><i></i>{playing ? 'On now' : 'Live'}</span>
      <span class="play" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.14v13.72a1 1 0 001.5.86l11-6.86a1 1 0 000-1.72l-11-6.86A1 1 0 008 5.14z"/></svg>
      </span>
    </span>
    <span class="name" dir="auto">{f.channel_name}</span>
    <span class="sub" dir="auto">{f.group_name}</span>
  </button>
{/snippet}

{#snippet poster(f: FavoriteChannel)}
  {@const t = splitTitle(f.channel_name)}
  {@const p = progress[f.channel_id]}
  <button class="poster" onclick={() => open(f)} aria-label={t.title}>
    <span class="art">
      {#if f.logo_url && !failed.has(f.channel_id)}
        <img src={f.logo_url} alt="" loading="lazy" onerror={() => (failed = new Set(failed).add(f.channel_id))} />
      {:else}
        <span class="art-empty" dir="auto">{t.title}</span>
      {/if}
      <span class="badge">{f.content_type === 'series' ? 'Series' : 'Movie'}</span>
      {#if continueAt[f.channel_id]}
        <span class="resume">Continue {continueAt[f.channel_id]}</span>
      {/if}
      {#if p}<span class="bar" aria-label={`${Math.round(p * 100)}% watched`}><span style:width="{p * 100}%"></span></span>{/if}
      <span class="play" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.14v13.72a1 1 0 001.5.86l11-6.86a1 1 0 000-1.72l-11-6.86A1 1 0 008 5.14z"/></svg>
      </span>
    </span>
    <span class="name" dir="auto">{t.title}</span>
    <span class="sub">{t.year ? `${t.year} · ` : ''}{addedAgo(f.added_at)}</span>
  </button>
{/snippet}

{#snippet itemTools(f: FavoriteChannel)}
  {@const own = lists.filter((l) => l.kind === f.content_type)}
  {@const cur = listOf(f)}
  <button
    class="sort-btn"
    class:has={!!cur}
    style:--c={cur ? colorOf(cur) : undefined}
    onclick={() => { menuFor = menuFor === f.channel_id ? null : f.channel_id; menuDraft = ''; }}
    aria-label="Category"
    aria-expanded={menuFor === f.channel_id}
    title={cur ? `Category: ${cur.name}` : 'Put in a category'}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round"><path d="M3 7.5A1.5 1.5 0 014.5 6h4.4l2 2.2h8.6A1.5 1.5 0 0121 9.7v8.8a1.5 1.5 0 01-1.5 1.5h-15A1.5 1.5 0 013 18.5z"/></svg>
  </button>
  <button class="remove" onclick={() => remove(f)} aria-label={`Remove ${f.channel_name} from My list`} title="Remove from My list">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
  </button>
  {#if menuFor === f.channel_id}
    <div class="menu" role="menu" aria-label="Category" transition:fly={{ y: -6, duration: 140 }}>
      <p class="menu-head">{KIND_LABEL[f.content_type]} category</p>
      {#each own as l (l.id)}
        <button role="menuitemradio" aria-checked={cur?.id === l.id} onclick={() => setList(f, l.id)}>
          <i class="dot" style:background={colorOf(l)}></i>
          <span dir="auto">{l.name}</span>
          {#if cur?.id === l.id}<svg class="tick" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.8" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>{/if}
        </button>
      {/each}
      {#if cur}
        <button role="menuitem" class="muted" onclick={() => setList(f, null)}>Take out of “{cur.name}”</button>
      {/if}
      <form class="menu-new" onsubmit={(e) => { e.preventDefault(); newListFor(f); }}>
        <input bind:value={menuDraft} placeholder={own.length ? 'New category…' : 'Name your first category…'} dir="auto" maxlength="40" aria-label="New category" />
        <button type="submit" disabled={!menuDraft.trim()} aria-label="Create and add">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
        </button>
      </form>
    </div>
  {/if}
{/snippet}

<div class="mylist">
  <header class="head">
    <div class="titles">
      <h1>My list</h1>
      {#if loaded}
        <p>
          {#if counts.all}
            {nf.format(counts.all)} saved
            {#if counts.live} · {nf.format(counts.live)} {counts.live === 1 ? 'channel' : 'channels'}{/if}
            {#if counts.vod} · {nf.format(counts.vod)} {counts.vod === 1 ? 'movie' : 'movies'}{/if}
            {#if counts.series} · {nf.format(counts.series)} series{/if}
          {:else}
            Everything you save shows up here
          {/if}
        </p>
      {/if}
    </div>

    {#if counts.all}
      <label class="search">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><line x1="16.5" y1="16.5" x2="21" y2="21"/></svg>
        <input bind:value={query} placeholder="Search my list" aria-label="Search my list" dir="auto" />
        {#if query}
          <button class="clear" onclick={() => (query = '')} aria-label="Clear">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
          </button>
        {/if}
      </label>
    {/if}
  </header>

  {#if loaded && counts.all}
    <div class="toolbar">
      <div class="tabs" role="tablist" aria-label="Type">
        {#each [['all', 'All'], ['live', 'Channels'], ['vod', 'Movies'], ['series', 'Series']] as [key, label] (key)}
          {@const n = counts[key as Tab]}
          <button role="tab" aria-selected={tab === key} class:on={tab === key} disabled={key !== 'all' && !n} onclick={() => (tab = key as Tab)}>
            {label}<span>{nf.format(n)}</span>
          </button>
        {/each}
      </div>
      <div class="right">
        <Dropdown
          label="Sort"
          value={sort}
          options={[{ value: 'added', label: 'Recently added' }, { value: 'name', label: 'A–Z' }]}
          onchange={(v) => (sort = v as Sort)}
        />
      </div>
    </div>

    {#if tab !== 'all'}
      <!-- Your categories for this type -->
      <div class="cats" role="toolbar" aria-label="{KIND_LABEL[tab]} categories">
        <button class="chip" class:on={listFilter === null} onclick={() => (listFilter = null)}>
          All <span>{nf.format(counts[tab])}</span>
        </button>
        {#each kindLists as l (l.id)}
          {#if draft?.id === l.id}
            <input class="chip-input" bind:this={draftEl} bind:value={draft.name} onkeydown={(e) => { if (e.key === 'Enter') saveDraft(); else if (e.key === 'Escape') draft = null; }} onblur={saveDraft} dir="auto" maxlength="40" aria-label="Category name" />
          {:else}
            <div class="chip-wrap" class:on={listFilter === l.id}>
              <button class="chip" class:on={listFilter === l.id} onclick={() => (listFilter = listFilter === l.id ? null : l.id)}>
                <i class="dot" style:background={colorOf(l)}></i>
                <b dir="auto">{l.name}</b>
                <span>{nf.format(listCounts.get(l.id) ?? 0)}</span>
              </button>
              <span class="chip-tools">
                {#if confirmDelete === l.id}
                  <button class="danger" onclick={() => deleteList(l)} onblur={() => (confirmDelete = null)}>Delete?</button>
                {:else}
                  <button onclick={() => startDraft(l.id, l.name)} aria-label="Rename {l.name}" title="Rename">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20h4L19 9l-4-4L4 16v4z"/></svg>
                  </button>
                  <button onclick={() => deleteList(l)} aria-label="Delete {l.name}" title="Delete category">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/></svg>
                  </button>
                {/if}
              </span>
            </div>
          {/if}
        {/each}
        {#if kindLists.length && listCounts.get('none')}
          <button class="chip" class:on={listFilter === 'none'} onclick={() => (listFilter = listFilter === 'none' ? null : 'none')}>
            Not sorted <span>{nf.format(listCounts.get('none') ?? 0)}</span>
          </button>
        {/if}
        {#if draft && draft.id === null}
          <input class="chip-input" bind:this={draftEl} bind:value={draft.name} onkeydown={(e) => { if (e.key === 'Enter') saveDraft(); else if (e.key === 'Escape') draft = null; }} onblur={saveDraft} placeholder="Category name" dir="auto" maxlength="40" aria-label="New category name" />
        {:else}
          <button class="chip add" onclick={() => startDraft(null)}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
            New category
          </button>
        {/if}
        {#if !kindLists.length && !draft}<p class="cats-hint">{KIND_HINT[tab]}</p>{/if}
      </div>
    {/if}
  {/if}

  {#if !loaded}
    <div class="grid posters" aria-hidden="true">
      {#each Array(12) as _, i (i)}<div class="sk poster-sk"></div>{/each}
    </div>
  {:else if !counts.all}
    <!-- First-time empty state -->
    <section class="empty" in:fade={{ duration: 200 }}>
      <div class="empty-art" aria-hidden="true">
        <span class="card c1"></span><span class="card c2"></span><span class="card c3"></span>
      </div>
      <h2>Your list is empty</h2>
      <p>Save the channels you watch every day and the movies and series you want to get to. They'll all be one click away here.</p>
      <ul class="how">
        <li><b>Channels:</b> hover a channel in Live TV and press <span class="k">♡</span></li>
        <li><b>Movies & series:</b> open one and press <span class="k">+</span> next to Play</li>
      </ul>
      <div class="cta">
        <button class="primary" onclick={() => goto('/live')}>Browse Live TV</button>
        <button onclick={() => goto('/browse?type=vod')}>Movies</button>
        <button onclick={() => goto('/browse?type=series')}>Series</button>
      </div>
    </section>
  {:else if !shown.length}
    <div class="none">
      <strong>Nothing here matches</strong>
      <span>{query ? `No saved titles contain “${query.trim()}”.` : 'No saved titles in this view.'}</span>
      <button onclick={() => { query = ''; tab = 'all'; listFilter = null; }}>Show everything</button>
    </div>
  {:else}
    {#each sections as sec (sec.kind)}
      <section class="block">
        {#if tab === 'all'}
          <h2>
            <button class="h2-link" onclick={() => (tab = sec.kind)}>{KIND_LABEL[sec.kind]} <span>{nf.format(sec.count)}</span></button>
          </h2>
        {/if}
        {#each sec.groups as g (g.list?.id ?? 'none')}
          {#if sec.grouped}
            <h3 class="group-head">
              {#if g.list}
                <i class="dot" style:background={colorOf(g.list)}></i>
                <button onclick={() => { const id = g.list!.id; tab = sec.kind; tick().then(() => (listFilter = id)); }} dir="auto">{g.list.name}</button>
              {:else}
                <span class="muted">Not sorted</span>
              {/if}
              <em>{nf.format(g.items.length)}</em>
            </h3>
          {/if}
          <div class="grid" class:tiles={sec.kind === 'live'} class:posters={sec.kind !== 'live'}>
            {#each g.items as f (f.channel_id)}
              <div class="item" class:menu-open={menuFor === f.channel_id} animate:flip={{ duration: 220 }} out:fade={{ duration: 150 }}>
                {#if sec.kind === 'live'}
                  {@render tile(f)}
                {:else}
                  {@render poster(f)}
                {/if}
                {@render itemTools(f)}
              </div>
            {/each}
          </div>
        {/each}
      </section>
    {/each}
  {/if}

  {#if note}
    <div class="toast" role="status" transition:fly={{ y: 16, duration: 180 }}><span>{note}</span></div>
  {/if}

  {#if undo}
    <div class="toast" role="status" transition:fly={{ y: 16, duration: 180 }}>
      <span>Removed <b dir="auto">{splitTitle(undo.channel_name).title}</b></span>
      <button onclick={restore}>Undo</button>
    </div>
  {/if}
</div>

<style>
  .mylist {
    --gutter: clamp(24px, 3.6vw, 60px);
    margin: -24px;
    min-height: 100vh;
    padding: 36px var(--gutter) 80px;
    background: radial-gradient(60% 40% at 85% 0%, oklch(0.4 0.15 27 / 0.16), transparent 70%);
  }

  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 22px;
  }
  .titles h1 { font-size: clamp(2.25rem, 3.6vw, 3.25rem); font-weight: 900; letter-spacing: -0.035em; }
  .titles p { margin-top: 4px; color: var(--color-text-muted); font-variant-numeric: tabular-nums; }

  .search {
    position: relative;
    display: flex;
    align-items: center;
    width: min(340px, 40vw);
    height: 44px;
    padding: 0 10px 0 40px;
    border-radius: 8px;
    background: oklch(1 0 0 / 0.06);
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.1);
    cursor: text;
  }
  .search:focus-within { box-shadow: inset 0 0 0 2px var(--color-text); }
  .search > :global(svg) { position: absolute; left: 13px; width: 18px; height: 18px; color: var(--color-text-muted); }
  .search input { flex: 1; min-width: 0; height: 100%; padding: 0; border: none; background: none; font-size: 0.9375rem; }
  .search input:focus { border: none; }
  .clear { width: 26px; height: 26px; display: grid; place-items: center; border-radius: 50%; background: oklch(1 0 0 / 0.1); color: var(--color-text); }
  .clear :global(svg) { width: 12px; height: 12px; }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
    margin-bottom: 28px;
  }
  .tabs { display: flex; gap: 6px; }
  .tabs button {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 16px;
    border-radius: 19px;
    background: oklch(1 0 0 / 0.06);
    color: oklch(0.85 0.004 25);
    font-size: 0.875rem;
    font-weight: 700;
    transition: background 150ms var(--ease-out), color 150ms var(--ease-out);
  }
  .tabs button span { font-size: 0.75rem; opacity: 0.6; font-variant-numeric: tabular-nums; }
  .tabs button:hover:not(:disabled) { background: oklch(1 0 0 / 0.12); }
  .tabs button.on { background: var(--color-text); color: oklch(0.14 0.004 25); }
  .tabs button:disabled { opacity: 0.35; }
  .tabs button:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  .right { display: flex; gap: 8px; --dd-bg: oklch(1 0 0 / 0.06); --dd-border: oklch(1 0 0 / 0.1); --dd-h: 38px; }

  /* Your categories */
  .cats { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: -12px 0 28px; }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 14px;
    border-radius: 17px;
    background: none;
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.14);
    color: oklch(0.85 0.004 25);
    font-size: 0.8125rem;
    font-weight: 700;
    transition: background 120ms var(--ease-out), color 120ms var(--ease-out);
  }
  .chip b { font-weight: 700; max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chip span { font-size: 0.75rem; opacity: 0.6; font-variant-numeric: tabular-nums; }
  .chip:hover { background: oklch(1 0 0 / 0.07); color: var(--color-text); }
  .chip.on { background: oklch(1 0 0 / 0.14); box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.3); color: var(--color-text); }
  .chip.add { box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.14); color: var(--color-text-muted); border: 1px dashed transparent; }
  .chip.add :global(svg) { width: 13px; height: 13px; }
  .chip-wrap { position: relative; display: inline-flex; }
  .chip-tools {
    position: absolute;
    right: 3px;
    top: 3px;
    bottom: 3px;
    display: none;
    gap: 2px;
    padding-left: 10px;
    border-radius: 0 15px 15px 0;
    background: linear-gradient(to right, transparent, oklch(0.25 0.005 25) 10px);
  }
  .chip-wrap:hover .chip-tools, .chip-wrap:focus-within .chip-tools { display: flex; }
  .chip-wrap:hover .chip span { visibility: hidden; }
  .chip-tools button {
    width: 28px;
    display: grid;
    place-items: center;
    border-radius: 14px;
    background: none;
    color: var(--color-text-muted);
  }
  .chip-tools button:hover { background: oklch(1 0 0 / 0.12); color: var(--color-text); }
  .chip-tools :global(svg) { width: 13px; height: 13px; }
  .chip-tools .danger { width: auto; padding: 0 10px; color: var(--color-accent-soft); font-size: 0.75rem; font-weight: 800; }
  .chip-input {
    height: 34px;
    width: 190px;
    padding: 0 14px;
    border: none;
    border-radius: 17px;
    background: var(--color-base);
    box-shadow: inset 0 0 0 2px var(--color-accent);
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 700;
    text-align: left;
  }
  .cats-hint { font-size: 0.8125rem; color: var(--color-text-muted); margin-left: 4px; }
  .dot { display: inline-block; flex: none; width: 9px; height: 9px; border-radius: 50%; }

  .block { margin-bottom: 44px; }
  .block h2 { font-size: 1.25rem; font-weight: 800; letter-spacing: -0.01em; margin-bottom: 14px; }
  .h2-link { display: inline-flex; align-items: baseline; gap: 10px; background: none; color: inherit; font: inherit; }
  .h2-link span { font-size: 0.875rem; font-weight: 600; color: var(--color-text-muted); }
  .h2-link:hover { text-decoration: underline; text-underline-offset: 4px; }
  .group-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 22px 0 12px;
    font-size: 1rem;
    font-weight: 800;
  }
  .block h2 + .group-head { margin-top: 4px; }
  .group-head button { background: none; color: inherit; font: inherit; text-align: left; }
  .group-head button:hover { text-decoration: underline; text-underline-offset: 3px; }
  .group-head em { font-style: normal; font-size: 0.8125rem; font-weight: 600; color: var(--color-text-muted); }
  .group-head .muted { color: var(--color-text-muted); }

  /* Category button + menu on each item */
  .sort-btn {
    position: absolute;
    top: 8px;
    right: 44px;
    z-index: 2;
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(0.12 0.004 25 / 0.85);
    color: var(--color-text);
    box-shadow: 0 0 0 1px oklch(1 0 0 / 0.2);
    opacity: 0;
    transform: scale(0.85);
    transition: opacity 150ms var(--ease-out), transform 150ms var(--ease-out), background 150ms var(--ease-out);
  }
  .sort-btn :global(svg) { width: 14px; height: 14px; }
  .sort-btn:hover { background: oklch(0.3 0.006 25); }
  .item:hover .sort-btn, .sort-btn:focus-visible, .item.menu-open .sort-btn { opacity: 1; transform: none; }
  .item.menu-open { z-index: 10; }
  .item.menu-open .remove { opacity: 1; transform: none; }
  .menu {
    position: absolute;
    top: 44px;
    right: 8px;
    z-index: 20;
    width: 240px;
    display: flex;
    flex-direction: column;
    padding: 6px;
    border-radius: 10px;
    background: oklch(0.22 0.005 25);
    box-shadow: 0 22px 48px -12px oklch(0 0 0 / 0.85), 0 0 0 1px oklch(1 0 0 / 0.1);
  }
  .menu-head { padding: 6px 8px 6px; font-size: 0.6875rem; font-weight: 800; letter-spacing: 0.06em; text-transform: uppercase; color: var(--color-text-muted); }
  .menu > button {
    display: flex;
    align-items: center;
    gap: 9px;
    min-height: 34px;
    padding: 0 9px;
    border-radius: 6px;
    background: none;
    color: oklch(0.9 0.004 25);
    font-size: 0.8125rem;
    font-weight: 600;
    text-align: left;
  }
  .menu > button span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .menu > button:hover { background: oklch(1 0 0 / 0.08); color: var(--color-text); }
  .menu > button.muted { color: var(--color-text-muted); }
  .tick { width: 15px; height: 15px; color: var(--color-accent-soft); }
  .menu-new { display: flex; gap: 4px; margin-top: 4px; padding-top: 6px; border-top: 1px solid oklch(1 0 0 / 0.08); }
  .menu-new input {
    flex: 1;
    min-width: 0;
    height: 32px;
    padding: 0 9px;
    border: none;
    border-radius: 6px;
    background: var(--color-base);
    box-shadow: inset 0 0 0 1px var(--color-border);
    font-size: 0.8125rem;
    text-align: left;
  }
  .menu-new input:focus { box-shadow: inset 0 0 0 2px var(--color-text-muted); }
  .menu-new button { width: 32px; height: 32px; display: grid; place-items: center; border-radius: 6px; background: var(--color-text); color: oklch(0.14 0.004 25); }
  .menu-new button:disabled { opacity: 0.35; }
  .menu-new :global(svg) { width: 14px; height: 14px; }

  .grid { display: grid; gap: 26px 14px; }
  .tiles { grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); }
  .posters { grid-template-columns: repeat(auto-fill, minmax(clamp(140px, 11vw, 180px), 1fr)); }

  .item { position: relative; min-width: 0; }

  .tile, .poster {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0;
    background: none;
    color: var(--color-text);
    text-align: start;
    border-radius: 8px;
  }
  .tile:focus-visible, .poster:focus-visible { outline: none; }
  .tile:focus-visible .tile-art, .poster:focus-visible .art { outline: 2px solid var(--color-text); outline-offset: 3px; }

  .tile-art, .art {
    position: relative;
    display: grid;
    place-items: center;
    border-radius: 8px;
    overflow: hidden;
    background: oklch(0.22 0.005 25);
    transition: transform 240ms var(--ease-out), box-shadow 240ms var(--ease-out);
  }
  .tile-art { aspect-ratio: 16 / 9; box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.06); }
  .tile-art img { width: 100%; height: 100%; object-fit: contain; padding: 16% 22%; }
  .tile.playing .tile-art { box-shadow: inset 0 0 0 2px var(--color-accent); }
  .letter { font-size: 2rem; font-weight: 900; color: var(--color-text-muted); }
  .art { aspect-ratio: 2 / 3; }
  .art img { width: 100%; height: 100%; object-fit: cover; }
  .art-empty {
    display: flex;
    align-items: flex-end;
    width: 100%;
    height: 100%;
    padding: 12px;
    font-weight: 800;
    background: radial-gradient(120% 80% at 100% 0%, oklch(0.42 0.16 27 / 0.55), transparent 60%), var(--color-card);
  }

  .tile:hover .tile-art, .poster:hover .art {
    transform: translateY(-4px);
    box-shadow: 0 20px 40px -16px oklch(0 0 0 / 0.85), 0 0 0 1px oklch(1 0 0 / 0.12);
  }

  .badge {
    position: absolute;
    top: 8px;
    left: 8px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 7px;
    border-radius: 3px;
    background: oklch(0.12 0.004 25 / 0.8);
    font-size: 0.625rem;
    font-weight: 800;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .badge.live { background: var(--color-accent); color: var(--color-on-accent); }
  .badge i { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }

  .resume {
    position: absolute;
    left: 8px;
    right: 8px;
    bottom: 12px;
    padding: 4px 8px;
    border-radius: 4px;
    background: oklch(0.12 0.004 25 / 0.85);
    font-size: 0.6875rem;
    font-weight: 800;
    text-align: center;
  }
  .bar { position: absolute; left: 0; right: 0; bottom: 0; height: 4px; background: oklch(1 0 0 / 0.25); }
  .bar span { display: block; height: 100%; background: var(--color-accent); }

  .play {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: oklch(0 0 0 / 0.35);
    opacity: 0;
    transition: opacity 180ms var(--ease-out);
  }
  .play :global(svg) {
    width: 46px;
    height: 46px;
    padding: 12px 11px 12px 13px;
    border-radius: 50%;
    background: oklch(0.98 0.004 25 / 0.95);
    color: oklch(0.14 0.004 25);
  }
  .tile:hover .play, .poster:hover .play, .tile:focus-visible .play, .poster:focus-visible .play { opacity: 1; }

  .name { font-size: 0.875rem; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sub { margin-top: -5px; font-size: 0.75rem; color: var(--color-text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .remove {
    position: absolute;
    top: 8px;
    right: 8px;
    z-index: 2;
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(0.12 0.004 25 / 0.85);
    color: var(--color-text);
    box-shadow: 0 0 0 1px oklch(1 0 0 / 0.2);
    opacity: 0;
    transform: scale(0.85);
    transition: opacity 150ms var(--ease-out), transform 150ms var(--ease-out), background 150ms var(--ease-out);
  }
  .remove :global(svg) { width: 13px; height: 13px; }
  .item:hover .remove, .remove:focus-visible { opacity: 1; transform: none; }
  .remove:hover { background: var(--color-accent); }
  .remove:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  /* Empty */
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 14px;
    max-width: 560px;
    margin: 6vh auto 0;
  }
  .empty-art { position: relative; width: 220px; height: 150px; margin-bottom: 10px; }
  .card { position: absolute; width: 92px; height: 132px; border-radius: 10px; background: oklch(0.24 0.005 25); box-shadow: 0 20px 40px -16px oklch(0 0 0 / 0.8), inset 0 0 0 1px oklch(1 0 0 / 0.06); }
  .c1 { left: 10px; top: 14px; transform: rotate(-10deg); opacity: 0.6; }
  .c3 { right: 10px; top: 14px; transform: rotate(10deg); opacity: 0.6; }
  .c2 { left: 64px; top: 0; z-index: 1; }
  .c2::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: 10px;
    background: radial-gradient(80% 60% at 50% 30%, oklch(0.58 0.225 27 / 0.45), transparent 70%);
  }
  .empty-art::after {
    content: '';
    position: absolute;
    left: 50%;
    top: 50%;
    width: 46px;
    height: 46px;
    translate: -50% -50%;
    z-index: 2;
    background: var(--color-accent);
    mask: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'%3E%3Cpath d='M12 21s-7.5-4.6-9.6-9.4C.9 8.1 3.2 4.5 6.9 4.5c2.1 0 3.6 1.1 5.1 3 1.5-1.9 3-3 5.1-3 3.7 0 6 3.6 4.5 7.1C19.5 16.4 12 21 12 21z'/%3E%3C/svg%3E") center / contain no-repeat;
  }
  .empty h2 { font-size: 2rem; font-weight: 900; letter-spacing: -0.03em; }
  .empty p { color: var(--color-text-muted); line-height: 1.6; }
  .how { list-style: none; display: flex; flex-direction: column; gap: 8px; font-size: 0.9375rem; color: oklch(0.85 0.004 25); }
  .how b { color: var(--color-text); }
  .k { display: inline-grid; place-items: center; min-width: 24px; height: 24px; padding: 0 6px; border-radius: 6px; background: oklch(1 0 0 / 0.1); font-weight: 800; }
  .cta { display: flex; flex-wrap: wrap; justify-content: center; gap: 10px; margin-top: 10px; }
  .cta button, .none button {
    height: 44px;
    padding: 0 20px;
    border-radius: 6px;
    background: oklch(1 0 0 / 0.1);
    color: var(--color-text);
    font-size: 0.9375rem;
    font-weight: 700;
    transition: background 150ms var(--ease-out);
  }
  .cta button:hover, .none button:hover { background: oklch(1 0 0 / 0.18); }
  .cta .primary { background: var(--color-text); color: oklch(0.14 0.004 25); }
  .cta .primary:hover { background: oklch(0.85 0.004 25); }

  .none { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 60px 0; text-align: center; }
  .none strong { font-size: 1.125rem; }
  .none span { color: var(--color-text-muted); margin-bottom: 8px; }

  .toast {
    position: fixed;
    left: 50%;
    bottom: 28px;
    z-index: 50;
    translate: -50% 0;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px 10px 10px 18px;
    border-radius: 8px;
    background: var(--color-text);
    color: oklch(0.14 0.004 25);
    font-size: 0.875rem;
    box-shadow: 0 20px 40px -12px oklch(0 0 0 / 0.7);
  }
  .toast button {
    height: 32px;
    padding: 0 14px;
    border-radius: 5px;
    background: oklch(0.14 0.004 25);
    color: var(--color-text);
    font-weight: 800;
  }

  .sk {
    border-radius: 8px;
    background: linear-gradient(90deg, oklch(1 0 0 / 0.05) 0%, oklch(1 0 0 / 0.1) 50%, oklch(1 0 0 / 0.05) 100%);
    background-size: 200% 100%;
    animation: shimmer 1.6s linear infinite;
  }
  .poster-sk { aspect-ratio: 2 / 3; }
  @keyframes shimmer { from { background-position: 100% 0; } to { background-position: -100% 0; } }

  @media (prefers-reduced-motion: reduce) {
    .sk { animation: none; }
    .tile-art, .art, .remove, .play { transition: none; }
  }
</style>
