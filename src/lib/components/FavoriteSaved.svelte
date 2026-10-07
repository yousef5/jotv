<script lang="ts" module>
  import { writable } from 'svelte/store';

  export interface FavoriteAsk {
    channelId: number;
    name: string;
    kind: 'live' | 'vod' | 'series';
    anchor: DOMRect;
  }

  /** Set right after something is added to favorites: offers a category for it */
  export const favoriteAsk = writable<FavoriteAsk | null>(null);

  /** The last favorite filed from this popover, so open pages can update */
  export const favoriteFiled = writable<{ channelId: number; listId: number | null; at: number } | null>(null);

  export function askFavoriteCategory(
    channel: { id: number; name: string; content_type: 'live' | 'vod' | 'series' },
    anchorEl: Element | null | undefined,
  ) {
    const anchor = anchorEl?.getBoundingClientRect() ?? new DOMRect(innerWidth / 2, 80, 0, 0);
    favoriteAsk.set({ channelId: channel.id, name: channel.name, kind: channel.content_type, anchor });
  }
</script>

<script lang="ts">
  import { tick } from 'svelte';
  import { fly } from 'svelte/transition';
  import { goto } from '$app/navigation';
  import { favoriteLists, favoriteListSave, favoriteSetList } from '$lib/tauri';
  import type { FavoriteList } from '$lib/tauri';
  import { videoAnchor, overlayOpen } from '$lib/stores/live';
  import { CATEGORY_COLORS } from '$lib/stores/reels';

  // "Saved ✓" popover next to the heart: pick one of your categories for the
  // new favorite, or make one. Ignored, it closes by itself.

  const AUTO_CLOSE_MS = 7000;
  const KIND: Record<FavoriteAsk['kind'], { label: string; hint: string }> = {
    live: { label: 'channel', hint: 'Sports, News, Kids…' },
    vod: { label: 'movie', hint: 'Comedy, Watch later…' },
    series: { label: 'series', hint: 'Watching now, Drama…' },
  };

  let ask = $state<FavoriteAsk | null>(null);
  let lists = $state<FavoriteList[]>([]);
  let chosen = $state<number | null>(null);
  let creating = $state(false);
  let newName = $state('');
  let inputEl = $state<HTMLInputElement | undefined>();
  let el = $state<HTMLDivElement | undefined>();
  let height = $state(0);
  let held = $state(false);
  let started = $state(0);
  let now = $state(0);
  let frame = 0;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;
  let coveredVideo = false;

  let own = $derived(ask ? lists.filter((l) => l.kind === ask!.kind) : []);
  let left = $derived(ask ? Math.min(innerWidth - 332, Math.max(12, ask.anchor.left + ask.anchor.width / 2 - 160)) : 0);
  let below = $derived(ask ? ask.anchor.bottom + 10 + height < innerHeight - 12 : true);
  let top = $derived(ask ? (below ? ask.anchor.bottom + 10 : Math.max(12, ask.anchor.top - 10 - height)) : 0);
  let remaining = $derived(held || chosen !== null ? 1 : Math.max(0, 1 - (now - started) / AUTO_CLOSE_MS));

  $effect(() => {
    return favoriteAsk.subscribe((a) => {
      if (a) open(a);
    });
  });

  async function open(a: FavoriteAsk) {
    close(false);
    ask = a;
    chosen = null;
    creating = false;
    newName = '';
    held = false;
    started = performance.now();
    favoriteAsk.set(null);
    tick_();
    lists = await favoriteLists().catch(() => []);
    await tick();
    avoidVideo();
  }

  function tick_() {
    cancelAnimationFrame(frame);
    const loop = () => {
      now = performance.now();
      if (!ask) return;
      if (!held && chosen === null && now - started > AUTO_CLOSE_MS) return close();
      frame = requestAnimationFrame(loop);
    };
    frame = requestAnimationFrame(loop);
  }

  // The video is a native surface above the page: never sit under it
  function avoidVideo() {
    const v = $videoAnchor?.el.getBoundingClientRect();
    const r = el?.getBoundingClientRect();
    if (!v || !r) return;
    const overlap = r.left < v.right && r.right > v.left && r.top < v.bottom && r.bottom > v.top;
    if (overlap) {
      coveredVideo = true;
      overlayOpen.set(true);
    }
  }

  function close(animate = true) {
    void animate;
    cancelAnimationFrame(frame);
    clearTimeout(closeTimer);
    ask = null;
    if (coveredVideo) {
      coveredVideo = false;
      overlayOpen.set(false);
    }
  }

  async function choose(id: number | null) {
    if (!ask) return;
    const a = ask;
    chosen = id;
    await favoriteSetList([a.channelId], id).catch(() => {});
    favoriteFiled.set({ channelId: a.channelId, listId: id, at: Date.now() });
    clearTimeout(closeTimer);
    closeTimer = setTimeout(() => ask === a && close(), 1100);
  }

  async function startCreate() {
    creating = true;
    held = true;
    await tick();
    inputEl?.focus();
  }

  async function create() {
    if (!ask || !newName.trim()) return;
    try {
      const l = await favoriteListSave(null, newName, ask.kind);
      lists = [...lists.filter((x) => x.id !== l.id), l];
      creating = false;
      newName = '';
      await choose(l.id);
    } catch {
      // Keep the field so the name can be fixed
    }
  }

  function colorOf(l: FavoriteList): string {
    return l.color ?? CATEGORY_COLORS[l.id % CATEGORY_COLORS.length];
  }

  function onKey(e: KeyboardEvent) {
    if (ask && e.key === 'Escape') {
      e.stopPropagation();
      close();
    }
  }

  function outside(e: PointerEvent) {
    if (ask && el && !el.contains(e.target as Node)) close();
  }
</script>

<svelte:window onkeydowncapture={onKey} onpointerdown={outside} onresize={() => ask && close()} />

{#if ask}
  {@const done = chosen !== null ? own.find((l) => l.id === chosen) : null}
  <div
    class="saved"
    class:up={!below}
    bind:this={el}
    bind:clientHeight={height}
    style:left="{left}px"
    style:top="{top}px"
    role="dialog"
    aria-label="Saved to My list"
    tabindex="-1"
    onpointerenter={() => (held = true)}
    onfocusin={() => (held = true)}
    transition:fly={{ y: below ? -8 : 8, duration: 180 }}
  >
    <div class="head">
      <span class="heart" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 21s-7.5-4.6-9.6-9.4C.9 8.1 3.2 4.5 6.9 4.5c2.1 0 3.6 1.1 5.1 3 1.5-1.9 3-3 5.1-3 3.7 0 6 3.6 4.5 7.1C19.5 16.4 12 21 12 21z"/></svg>
      </span>
      <div class="titles">
        <b>{done ? `Saved to ${done.name}` : 'Saved to My list'}</b>
        <span dir="auto">{ask.name}</span>
      </div>
      <button class="x" onclick={() => close()} aria-label="Close">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/></svg>
      </button>
    </div>

    <p class="ask">Put this {KIND[ask.kind].label} in a category</p>
    <div class="chips">
      {#each own as l (l.id)}
        <button class="chip" class:on={chosen === l.id} onclick={() => choose(l.id)} aria-pressed={chosen === l.id}>
          <i style:background={colorOf(l)}></i>
          <span dir="auto">{l.name}</span>
          {#if chosen === l.id}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
          {/if}
        </button>
      {/each}
      {#if creating}
        <form class="new" onsubmit={(e) => { e.preventDefault(); create(); }}>
          <input bind:this={inputEl} bind:value={newName} placeholder={KIND[ask.kind].hint} dir="auto" maxlength="40" aria-label="New category name" />
          <button type="submit" disabled={!newName.trim()}>Add</button>
        </form>
      {:else}
        <button class="chip add" onclick={startCreate}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
          New category
        </button>
      {/if}
    </div>

    <div class="foot">
      <button class="link" onclick={() => { close(); goto('/favorites'); }}>Open My list</button>
      <button class="done" onclick={() => close()}>{chosen !== null ? 'Done' : 'Skip'}</button>
    </div>
    <span class="timer" aria-hidden="true"><i style:transform="scaleX({remaining})"></i></span>
  </div>
{/if}

<style>
  .saved {
    position: fixed;
    z-index: 400;
    width: 320px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 14px 12px;
    border-radius: 14px;
    overflow: hidden;
    background: oklch(0.21 0.006 25);
    box-shadow: 0 28px 60px -16px oklch(0 0 0 / 0.9), 0 0 0 1px oklch(1 0 0 / 0.1);
  }
  .saved:focus { outline: none; }

  .head { display: flex; align-items: center; gap: 11px; }
  .heart {
    flex: none;
    width: 38px;
    height: 38px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(0.58 0.225 27 / 0.18);
    color: var(--color-accent);
    animation: pop 420ms var(--ease-out);
  }
  .heart :global(svg) { width: 19px; height: 19px; }
  @keyframes pop {
    0% { transform: scale(0.4); }
    60% { transform: scale(1.12); }
    100% { transform: scale(1); }
  }
  .titles { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .titles b { font-size: 0.9375rem; font-weight: 800; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .titles span { font-size: 0.75rem; color: var(--color-text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; text-align: left; }
  .x { flex: none; width: 28px; height: 28px; display: grid; place-items: center; border-radius: 50%; background: none; color: var(--color-text-muted); }
  .x:hover { background: oklch(1 0 0 / 0.1); color: var(--color-text); }
  .x :global(svg) { width: 13px; height: 13px; }

  .ask { font-size: 0.75rem; font-weight: 700; color: var(--color-text-muted); }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    max-width: 100%;
    padding: 0 12px;
    border-radius: 16px;
    background: oklch(1 0 0 / 0.07);
    color: oklch(0.9 0.004 25);
    font-size: 0.8125rem;
    font-weight: 700;
    transition: background 120ms var(--ease-out), color 120ms var(--ease-out), transform 120ms var(--ease-out);
  }
  .chip span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chip i { flex: none; width: 8px; height: 8px; border-radius: 50%; }
  .chip:hover { background: oklch(1 0 0 / 0.13); color: var(--color-text); }
  .chip:active { transform: scale(0.96); }
  .chip.on { background: var(--color-text); color: oklch(0.16 0.004 25); }
  .chip :global(svg) { flex: none; width: 13px; height: 13px; }
  .chip.add { background: none; box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.18); color: var(--color-text-muted); }
  .chip.add:hover { color: var(--color-text); box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.35); }
  .chip:focus-visible, .done:focus-visible, .link:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  .new { flex: 1 1 100%; display: flex; gap: 6px; }
  .new input {
    flex: 1;
    min-width: 0;
    height: 34px;
    padding: 0 12px;
    border: none;
    border-radius: 17px;
    background: var(--color-base);
    box-shadow: inset 0 0 0 2px var(--color-accent);
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 600;
    text-align: left;
  }
  .new button {
    height: 34px;
    padding: 0 14px;
    border-radius: 17px;
    background: var(--color-accent);
    color: var(--color-on-accent);
    font-size: 0.8125rem;
    font-weight: 800;
  }
  .new button:disabled { opacity: 0.4; }

  .foot { display: flex; align-items: center; justify-content: space-between; margin-top: 2px; }
  .link { background: none; color: var(--color-text-muted); font-size: 0.75rem; font-weight: 700; }
  .link:hover { color: var(--color-text); text-decoration: underline; text-underline-offset: 3px; }
  .done {
    height: 32px;
    padding: 0 16px;
    border-radius: 7px;
    background: oklch(1 0 0 / 0.1);
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 800;
  }
  .done:hover { background: oklch(1 0 0 / 0.18); }

  /* Time left before it closes by itself */
  .timer { position: absolute; left: 0; right: 0; bottom: 0; height: 2px; background: oklch(1 0 0 / 0.06); }
  .timer i { display: block; height: 100%; background: var(--color-accent); transform-origin: left; }

  @media (prefers-reduced-motion: reduce) {
    .heart { animation: none; }
  }
</style>
