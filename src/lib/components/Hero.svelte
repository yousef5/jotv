<script lang="ts" module>
  import type { Channel } from '$lib/tauri';

  interface HeroInfo {
    backdrop: string | null;
    plot: string | null;
    genres: string[];
    year: string | null;
    rating: number | null;
    runtime: string | null;
    /** TMDB title treatment, when a key is set */
    logo?: string | null;
    /** Backdrop is decoded and landscape, so it can fill the billboard */
    wide?: boolean;
  }

  // Survive navigation so returning to a page doesn't re-hit the Xtream API
  // (the provider IP-blocks request bursts). Shared by every billboard.
  const heroInfoCache = new Map<number, HeroInfo | null>();
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { goto } from '$app/navigation';
  import { splitTitle, toRating, formatRuntime, backdropUrl, isWideBackdrop, searchTitle, fixArabic } from '$lib/playback';
  import { getVodInfo, getSeriesInfo, tmdbDetails, isFavorite, toggleFavorite } from '$lib/tauri';
  import { playlists } from '$lib/stores/playlists';

  let {
    items,
    index = $bindable(0),
    label = 'Featured',
    kicker = (c: Channel) => `New ${c.content_type === 'series' ? 'series' : 'film'} on`,
    children,
    fallback,
  }: {
    /** Titles to rotate through, newest first */
    items: Channel[];
    index?: number;
    label?: string;
    /** Line above the title, followed by the JTV mark */
    kicker?: (c: Channel) => string;
    /** Overlays drawn on top of the billboard (top bar, alerts) */
    children?: Snippet;
    /** Shown when there is nothing to feature */
    fallback?: Snippet;
  } = $props();

  const HERO_SECONDS = 9;

  let heroInfo = $state<Record<number, HeroInfo | null>>({});
  /** Titles whose billboard art is still being fetched: show only the ambient glow meanwhile */
  let heroPending = $state<Record<number, boolean>>({});
  /** Whether each billboard title is in My list */
  let listed = $state<Record<number, boolean>>({});

  let current = $derived(items[index] ?? null);

  // Off screen: hold the slide, so no crossfade repaints the big layers while rows scroll
  let heroEl = $state<HTMLElement | undefined>();
  let offscreen = $state(false);
  $effect(() => {
    if (!heroEl) return;
    const io = new IntersectionObserver(([e]) => (offscreen = !e.isIntersecting), {
      root: document.querySelector('main'),
      threshold: 0.25,
    });
    io.observe(heroEl);
    return () => io.disconnect();
  });

  $effect(() => {
    if (index >= items.length) index = 0;
    const cur = items[index];
    const next = items[(index + 1) % items.length];
    if (cur) {
      ensureHeroInfo(cur);
      if (!(cur.id in listed)) isFavorite(cur.id).then((v) => (listed = { ...listed, [cur.id]: v })).catch(() => {});
    }
    if (next && next !== cur) ensureHeroInfo(next);
  });

  async function ensureHeroInfo(ch: Channel) {
    if (ch.id in heroInfo) return;
    if (heroInfoCache.has(ch.id)) {
      heroInfo = { ...heroInfo, [ch.id]: heroInfoCache.get(ch.id) ?? null };
      return;
    }
    heroInfo = { ...heroInfo, [ch.id]: null };
    heroPending = { ...heroPending, [ch.id]: true };
    try {
      await fetchHeroInfo(ch);
    } finally {
      heroPending = { ...heroPending, [ch.id]: false };
    }
  }

  /** Loads and decodes an image so the slide can swap in without a pop */
  async function probeWide(url: string): Promise<boolean> {
    const img = new Image();
    img.src = url;
    await img.decode().catch(() => {});
    return isWideBackdrop(img);
  }

  async function fetchHeroInfo(ch: Channel) {
    const p = $playlists.find((pl) => pl.id === ch.playlist_id);
    if (!p || p.source_type !== 'xtream' || !p.source_url || !p.xtream_username || !p.xtream_password) return;

    const last = ch.stream_url.split('/').pop() ?? '';
    const remoteId = parseInt(last.split('.')[0]);
    if (isNaN(remoteId)) return;

    try {
      const raw = ch.content_type === 'series'
        ? (await getSeriesInfo(p.source_url, p.xtream_username, p.xtream_password, remoteId)).info
        : (await getVodInfo(p.source_url, p.xtream_username, p.xtream_password, remoteId)).info;
      const durationSecs = 'duration_secs' in raw ? (raw.duration_secs as number | null) : null;
      const info: HeroInfo = {
        backdrop: backdropUrl(raw.backdrop_path, raw.cover),
        plot: raw.plot?.trim() || null,
        genres: (raw.genre ?? '').split(/[,/]/).map((g) => g.trim()).filter(Boolean).slice(0, 3),
        year: raw.release_date?.slice(0, 4) || null,
        rating: toRating(raw.rating),
        runtime: formatRuntime(durationSecs),
      };
      // TMDB (optional): sharper backdrop and the title logo. The provider's ids pin the exact title.
      const year = splitTitle(ch.name).year ?? info.year;
      const t = await tmdbDetails(ch.content_type as 'vod' | 'series', searchTitle(ch.name), year ? +year : null, {
        tmdbId: raw.tmdb_id,
        tmdbHint: 'tmdb_hint' in raw ? raw.tmdb_hint : null,
      }).catch(() => null);
      if (t) {
        // Full size: the billboard spans the whole window
        info.backdrop = t.backdrop?.replace('/t/p/w1280/', '/t/p/original/') ?? info.backdrop;
        info.logo = t.logo;
        info.plot ??= t.overview;
      }
      if (info.backdrop) info.wide = await probeWide(info.backdrop);
      heroInfoCache.set(ch.id, info);
      heroInfo = { ...heroInfo, [ch.id]: info };
    } catch {
      heroInfoCache.set(ch.id, null);
    }
  }

  async function toggleListed(ch: Channel) {
    const v = await toggleFavorite(ch.id).catch(() => listed[ch.id]);
    listed = { ...listed, [ch.id]: v };
  }

  // Crossfade: the new slide fades in over the old one, which stays fully
  // opaque underneath (fading both dips the brightness mid-swap).
  // Driven per frame from script (`tick`, not `css`): WebKitGTK glitches a frame
  // whenever a GPU opacity/transform animation ends, so nothing here hands one off.
  const SWAP_MS = 700;
  function reveal(node: HTMLElement) {
    node.style.zIndex = '1';
    return {
      duration: SWAP_MS,
      tick: (t: number) => {
        node.style.opacity = String(t);
        if (t === 1) node.style.zIndex = '';
      },
    };
  }
  function hold(node: HTMLElement) {
    node.style.zIndex = '0';
    return { duration: SWAP_MS, tick: () => {} };
  }

  function advanceHero() {
    if (items.length > 1) index = (index + 1) % items.length;
  }

  /** Movies play in the app with 'play'; series and 'detail' open the title page */
  function open(ch: Channel, action: 'detail' | 'play' = 'detail') {
    goto(action === 'play' && ch.content_type === 'vod' ? `/watch?id=${ch.id}` : `/title?id=${ch.id}`);
  }
</script>

<section class="hero dark-zone" class:offscreen bind:this={heroEl} aria-roledescription="carousel" aria-label={label}>
  {@render children?.()}

  {#if current}
    <!-- An each over one slide (not #key) so the outgoing slide keeps its own
         image while it fades; with #key it would read the new title mid-fade. -->
    {#each [current] as slide (slide.id)}
      {@const info = heroInfo[slide.id]}
      <div class="hero-media" class:poster-led={!info?.wide} in:reveal out:hold>
        {#if info?.wide}
          <img class="hero-backdrop" src={info.backdrop} alt="" />
        {:else}
          <!-- The 80px blur is costly; only paint it when no backdrop covers it -->
          <img class="hero-ambient" src={slide.logo_url} alt="" aria-hidden="true" />
        {/if}
        {#if !info?.wide && !heroPending[slide.id]}
          <img class="hero-art" src={slide.logo_url} alt="" />
        {/if}
        <div class="hero-shade" aria-hidden="true"></div>
      </div>
    {/each}

    <!-- Same trick as the media: the outgoing copy keeps its own title while it fades -->
    {#each [current] as slide (slide.id)}
      {@const info = heroInfo[slide.id]}
      {@const name = splitTitle(slide.name)}
      <div class="hero-copy">
        <span class="kicker"><i class="new-dot" aria-hidden="true"></i>{kicker(slide)} <b>J</b>TV</span>
        {#if info?.logo}
          <h1 class="sr-only">{name.title}</h1>
          <img class="hero-logo" src={info.logo} alt="" />
        {:else}
          <h1 class="hero-title" dir="auto">{name.title}</h1>
        {/if}
        <div class="hero-meta">
          {#if info?.rating}
            <span class="score">
              <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 2.8l2.8 5.9 6.4.8-4.7 4.4 1.2 6.4L12 17.2l-5.7 3.1 1.2-6.4-4.7-4.4 6.4-.8z"/></svg>
              {info.rating.toFixed(1)}
            </span>
          {/if}
          {#if info?.year || name.year}<span>{info?.year ?? name.year}</span>{/if}
          {#if info?.runtime}<span>{info.runtime}</span>{/if}
          {#if info?.genres.length}<span class="genres">{info.genres.join(' · ')}</span>{/if}
        </div>
        {#if info?.plot}
          <p class="hero-plot" dir="auto">{fixArabic(info.plot)}</p>
        {:else}
          <p class="hero-plot muted" dir="auto">{slide.group_name}</p>
        {/if}
        <div class="hero-actions">
          <button class="btn-play" onclick={() => open(slide, slide.content_type === 'series' ? 'detail' : 'play')}>
            <svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 4.5v15a1 1 0 001.53.85l12-7.5a1 1 0 000-1.7l-12-7.5A1 1 0 007 4.5z"/></svg>
            {slide.content_type === 'series' ? 'Watch now' : 'Play'}
          </button>
          <button
            class="btn-round"
            class:on={listed[slide.id]}
            onclick={() => toggleListed(slide)}
            aria-label={listed[slide.id] ? 'Remove from My list' : 'Add to My list'}
            title={listed[slide.id] ? 'In My list' : 'Add to My list'}
          >
            {#if listed[slide.id]}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="5 12.5 10 17.5 19 7"/></svg>
            {:else}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
            {/if}
          </button>
          <button class="btn-round" onclick={() => open(slide, 'detail')} aria-label="More info" title="More info">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9.5"/><line x1="12" y1="11" x2="12" y2="16.5"/><circle cx="12" cy="7.75" r="0.6" fill="currentColor"/></svg>
          </button>
        </div>
      </div>
    {/each}

    {#if items.length > 1}
      <div class="picker" role="tablist" aria-label={label}>
        <span class="timer" aria-hidden="true">
          {#key index}<span class="timer-fill" style:animation-duration="{HERO_SECONDS}s" onanimationend={advanceHero}></span>{/key}
        </span>
        {#each items as f, i (f.id)}
          <button
            class="pick"
            class:active={i === index}
            role="tab"
            aria-selected={i === index}
            aria-label={`Show ${splitTitle(f.name).title}`}
            title={splitTitle(f.name).title}
            onclick={() => (index = i)}
          >
            <img src={f.logo_url} alt="" loading="lazy" />
          </button>
        {/each}
      </div>
    {/if}
  {:else}
    {@render fallback?.()}
  {/if}
</section>

<style>
  .kicker {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    font-size: 0.75rem;
    font-weight: 700;
    letter-spacing: 0.32em;
    color: var(--color-text);
    opacity: 0.9;
  }
  .kicker b {
    font-size: 1.5rem;
    font-weight: 900;
    letter-spacing: -0.04em;
    color: var(--color-accent);
  }
  .hero-copy .kicker {
    align-items: center;
    gap: 8px;
    font-size: 0.6875rem;
    letter-spacing: 0.24em;
    text-transform: uppercase;
  }
  .hero-copy .kicker b { font-size: 1.125rem; margin-right: -6px; }
  .new-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-accent);
    box-shadow: 0 0 0 4px oklch(0.58 0.225 27 / 0.25);
  }

  .btn-play {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    height: 58px;
    min-width: 230px;
    padding: 0 34px 0 28px;
    border-radius: 999px;
    background: var(--color-text);
    color: oklch(var(--n140));
    font-size: 1.125rem;
    font-weight: 800;
    box-shadow: 0 12px 30px -10px oklch(0 0 0 / 0.6);
    transition: background 200ms var(--ease-out);
  }
  .btn-play :global(svg) { width: 24px; height: 24px; }
  .btn-play:hover { background: oklch(var(--n860)); }

  .btn-round {
    width: 58px;
    height: 58px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(var(--n220) / 0.82);
    box-shadow: inset 0 0 0 1px oklch(var(--n1000) / 0.16);
    color: var(--color-text);
    transition: background 200ms var(--ease-out);
  }
  .btn-round :global(svg) { width: 24px; height: 24px; }
  .btn-round:hover { background: oklch(var(--n420) / 0.7); }
  .btn-round.on { background: var(--color-text); color: oklch(var(--n140)); }

  .btn-play:active,
  .btn-round:active { transform: scale(0.96); }
  .btn-play:focus-visible,
  .btn-round:focus-visible { outline: 2px solid var(--color-text); outline-offset: 3px; }

  .hero {
    --gutter: clamp(24px, 3.6vw, 60px);
    position: relative;
    /* Full window; the first row peeks in at the bottom */
    height: max(560px, 100vh);
    overflow: hidden;
    isolation: isolate;
  }

  .hero-media {
    position: absolute;
    inset: 0;
    z-index: -1;
  }

  .hero-ambient {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    filter: blur(80px) saturate(1.6) brightness(0.62);
    transform: scale(1.4);
  }

  .hero-backdrop {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center 20%;

  }

  /* No wide backdrop: show the poster sharp at a size it holds up at, over its own glow */
  .hero-art {
    position: absolute;
    right: calc(var(--gutter) + 3vw);
    top: 96px;
    height: min(calc(100% - 96px - 168px), 640px);
    aspect-ratio: 2 / 3;
    object-fit: cover;
    border-radius: 12px;
    box-shadow: 0 40px 90px -24px oklch(0 0 0 / 0.85), 0 0 0 1px oklch(var(--n1000) / 0.12);
  }

  .hero-shade {
    position: absolute;
    inset: 0;
    background:
      linear-gradient(to top, var(--color-base) 0%, oklch(var(--n165) / 0.6) 18%, transparent 42%),
      linear-gradient(77deg, oklch(var(--n165) / 0.92) 0%, oklch(var(--n165) / 0.55) 38%, transparent 68%),
      linear-gradient(to bottom, oklch(var(--n100) / 0.55) 0%, transparent 18%);
  }

  .hero-media.poster-led .hero-shade {
    background:
      linear-gradient(to top, var(--color-base) 0%, oklch(var(--n165) / 0.5) 22%, transparent 48%),
      linear-gradient(90deg, oklch(var(--n165) / 0.8) 0%, oklch(var(--n165) / 0.35) 40%, transparent 60%);
  }

  .hero-copy {
    /* Swaps instantly with the slide: a fading text layer is exactly what
       WebKitGTK glitches on */
    position: absolute;
    left: var(--gutter);
    bottom: 84px;
    width: min(600px, 46vw);
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
  }

  .hero-title {
    font-size: clamp(2.5rem, 5vw, 4.75rem);
    font-weight: 900;
    line-height: 1.04;
    letter-spacing: -0.035em;
    color: var(--color-text);
    text-wrap: balance;
    text-shadow: 0 2px 24px oklch(0 0 0 / 0.45);
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .hero-logo {
    max-width: min(540px, 92%);
    max-height: clamp(120px, 18vh, 200px);
    object-fit: contain;
    object-position: left bottom;
    filter: drop-shadow(0 6px 24px oklch(0 0 0 / 0.55));
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }

  .hero-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    font-size: 0.9375rem;
    font-weight: 500;
    color: oklch(var(--n880));
  }
  .hero-meta .score {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 9px 2px 7px;
    border-radius: 999px;
    background: oklch(0.78 0.15 75 / 0.16);
    color: var(--color-accent-yellow);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .hero-meta .score :global(svg) { width: 14px; height: 14px; }
  .hero-meta .genres { color: var(--color-text-muted); }

  .hero-plot {
    font-size: 1.0625rem;
    line-height: 1.5;
    color: oklch(var(--n900));
    max-width: 58ch;
    /* Shrink to the text so an Arabic plot hugs the copy column too */
    width: fit-content;
    text-shadow: 0 1px 12px oklch(0 0 0 / 0.5);
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .hero-plot:dir(rtl) { font-size: 1.125rem; font-weight: 300; line-height: 1.9; }
  .hero-title:dir(rtl) { line-height: 1.25; letter-spacing: 0; }
  .hero-plot.muted { color: var(--color-text-muted); }

  .hero-actions {
    display: flex;
    gap: 12px;
    margin-top: 8px;
  }

  .picker {
    position: absolute;
    right: var(--gutter);
    bottom: 92px;
    z-index: 3;
    display: flex;
    align-items: center;
    gap: 14px;
  }
  /* Every pick has the same box; the active one fills it, the rest sit inset.
     Paint-only: no GPU transform to glitch, and the neighbours never shift. */
  .pick {
    position: relative;
    width: 80px;
    height: 80px;
    padding: 0;
    border-radius: 50%;
    background: transparent;
  }
  .pick img {
    position: absolute;
    inset: 7px;
    width: calc(100% - 14px);
    height: calc(100% - 14px);
    object-fit: cover;
    object-position: center 20%;
    border-radius: 50%;
    box-shadow: 0 10px 24px -8px oklch(0 0 0 / 0.7);
  }
  .pick::after {
    content: '';
    position: absolute;
    inset: 7px;
    border-radius: 50%;
    background: oklch(var(--n100) / 0.45);
  }
  .pick:hover::after { background: oklch(var(--n100) / 0.15); }
  .pick.active img {
    inset: 0;
    width: 100%;
    height: 100%;
    box-shadow: 0 0 0 2px var(--color-text), 0 14px 30px -8px oklch(0 0 0 / 0.8);
  }
  .pick.active::after { display: none; }
  /* WebKitGTK draws a square native ring on click; use a round one for keyboard focus only */
  .pick:focus,
  .pick:focus-visible { outline: none; }
  .pick:focus-visible:not(.active) img { box-shadow: 0 0 0 2px var(--color-text); }

  /* Slide timer: a hairline across the picker */
  .timer {
    position: absolute;
    left: 0;
    right: 0;
    bottom: -22px;
    height: 2px;
    border-radius: 1px;
    overflow: hidden;
    background: oklch(var(--n1000) / 0.16);
  }
  .timer-fill {
    position: absolute;
    inset: 0;
    background: var(--color-text);
    /* width, not scaleX: a paint animation, so its end can't glitch */
    right: auto;
    width: 0;
    animation: fill linear forwards;
  }
  /* Pause for keyboard focus only: a mouse click leaves focus on the button and
     :focus-within would then hold the slide forever */
  /* Hovering anywhere can't pause it: the billboard fills the window, so the
     mouse is almost always over it. Only the picker holds the slide. */
  .hero.offscreen .timer-fill,
  .picker:hover .timer-fill,
  .hero:has(:focus-visible) .timer-fill { animation-play-state: paused; }
  @keyframes fill {
    from { width: 0; }
    to { width: 100%; }
  }

  /* Laptops: smaller picks so they never reach the copy */
  @media (max-width: 1440px) {
    .picker { gap: 10px; }
    .pick { width: 60px; height: 60px; }
    .pick img, .pick::after { inset: 5px; }
    .pick img { width: calc(100% - 10px); height: calc(100% - 10px); }
  }
  /* Narrow windows: the picks move under the copy. Never hide them: the
     timer inside drives the slides, so a hidden picker would stop the hero. */
  @media (max-width: 1180px) {
    .hero-copy { bottom: 132px; width: min(600px, calc(100% - 2 * var(--gutter))); }
    .picker { right: auto; left: var(--gutter); bottom: 48px; }
  }



  @media (prefers-reduced-motion: reduce) {
    .timer-fill { animation: none; width: 100%; }
  }
</style>
