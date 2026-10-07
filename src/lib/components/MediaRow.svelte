<script lang="ts" module>
  export interface RowItem {
    key: string | number;
    title: string;
    subtitle?: string;
    image: string | null;
    live?: boolean;
  }

  // Horizontal position per row title, so rows stay where you left them
  const rowPositions = new Map<string, number>();
</script>

<script lang="ts">
  let {
    title,
    items,
    variant = 'poster',
    onselect,
    onviewall,
  }: {
    title: string;
    items: RowItem[];
    variant?: 'poster' | 'landscape';
    onselect: (index: number) => void;
    onviewall?: () => void;
  } = $props();

  let scroller = $state<HTMLDivElement | undefined>();
  let atStart = $state(true);
  let atEnd = $state(false);
  let failed = $state<Set<string | number>>(new Set());

  function onScroll() {
    if (scroller) rowPositions.set(title, scroller.scrollLeft);
    updateEdges();
  }

  function updateEdges() {
    if (!scroller) return;
    atStart = scroller.scrollLeft <= 4;
    atEnd = scroller.scrollLeft + scroller.clientWidth >= scroller.scrollWidth - 4;
  }

  function page(dir: 1 | -1) {
    scroller?.scrollBy({ left: dir * scroller.clientWidth * 0.85, behavior: 'smooth' });
  }

  function markFailed(key: string | number) {
    failed = new Set(failed).add(key);
  }

  let restored = false;

  $effect(() => {
    items;
    queueMicrotask(() => {
      const saved = rowPositions.get(title);
      if (!restored && scroller && saved) {
        restored = true;
        scroller.scrollLeft = saved;
      }
      updateEdges();
    });
  });
</script>

<svelte:window onresize={updateEdges} />

<section class="row">
  <header class="row-head">
    {#if onviewall}
      <h2 class="row-title"><button class="row-link" onclick={onviewall}>{title}</button></h2>
    {:else}
      <h2 class="row-title">{title}</h2>
    {/if}
    {#if onviewall}
      <button class="row-more" onclick={onviewall}>
        Explore all
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="9 6 15 12 9 18"/></svg>
      </button>
    {/if}
  </header>

  <div class="row-body">
    {#if !atStart}
      <button class="pager prev" aria-label="Scroll left" onclick={() => page(-1)}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="15 6 9 12 15 18"/></svg>
      </button>
    {/if}

    <div class="track" class:landscape={variant === 'landscape'} bind:this={scroller} onscroll={onScroll}>
      {#each items as item, i (item.key)}
        {@const hasImage = item.image && !failed.has(item.key)}
        <button class="tile" aria-label={item.title} onclick={() => onselect(i)}>
          <div class="art">
            {#if hasImage}
              {#if variant === 'landscape'}
                <img class="art-blur" src={item.image} alt="" aria-hidden="true" loading="lazy" />
                <img class="art-fit" class:logo={item.live} src={item.image} alt="" loading="lazy" onerror={() => markFailed(item.key)} />
              {:else}
                <img class="art-cover" src={item.image} alt="" loading="lazy" onerror={() => markFailed(item.key)} />
              {/if}
            {:else}
              <div class="art-empty"><span dir="auto">{item.title}</span></div>
            {/if}

            {#if item.live}
              <span class="live-pill"><i></i>Live</span>
            {/if}

            <div class="hover-info">
              <span class="play-dot">
                <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.14v13.72a1 1 0 001.5.86l11-6.86a1 1 0 000-1.72l-11-6.86A1 1 0 008 5.14z"/></svg>
              </span>
              {#if variant === 'poster'}
                <span class="hover-title" dir="auto">{item.title}</span>
                {#if item.subtitle}<span class="hover-sub" dir="auto">{item.subtitle}</span>{/if}
              {/if}
            </div>
          </div>

          {#if variant === 'landscape'}
            <span class="caption" dir="auto">{item.title}</span>
            {#if item.subtitle}<span class="caption-sub" dir="auto">{item.subtitle}</span>{/if}
          {/if}
        </button>
      {/each}
    </div>

    {#if !atEnd}
      <button class="pager next" aria-label="Scroll right" onclick={() => page(1)}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="9 6 15 12 9 18"/></svg>
      </button>
    {/if}
  </div>
</section>

<style>
  .row {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .row-head {
    display: flex;
    align-items: baseline;
    gap: 14px;
    padding: 0 var(--gutter);
  }

  .row-title {
    font-size: 1.25rem;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: var(--color-text);
  }

  .row-link {
    padding: 0;
    background: none;
    color: inherit;
    font: inherit;
    letter-spacing: inherit;
    border-radius: 4px;
  }
  .row-link:hover { color: oklch(0.85 0.004 25); }
  .row-link:focus-visible { outline: 2px solid var(--color-text); outline-offset: 3px; }

  .row-more {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    background: none;
    padding: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--color-accent-soft);
    opacity: 0;
    transform: translateX(-6px);
    transition: opacity 200ms var(--ease-out), transform 200ms var(--ease-out);
  }
  .row-more :global(svg) { width: 14px; height: 14px; }
  .row:hover .row-more,
  .row-more:focus-visible { opacity: 1; transform: none; }

  .row-body { position: relative; }

  .track {
    --tile-w: clamp(132px, 10.5vw, 188px);
    display: flex;
    gap: 8px;
    overflow-x: auto;
    overflow-y: hidden;
    scroll-snap-type: x proximity;
    scroll-padding-inline: var(--gutter);
    padding: 18px var(--gutter) 22px;
    scrollbar-width: none;
  }
  .track::-webkit-scrollbar { display: none; }
  .track.landscape { --tile-w: clamp(220px, 16.5vw, 300px); }

  .tile {
    flex: 0 0 var(--tile-w);
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0;
    background: none;
    color: inherit;
    text-align: start;
    scroll-snap-align: start;
    border-radius: 6px;
  }

  .art {
    position: relative;
    width: 100%;
    aspect-ratio: 2 / 3;
    border-radius: 6px;
    overflow: hidden;
    background: var(--color-card);
    transition: transform 240ms var(--ease-out), box-shadow 240ms var(--ease-out);
  }
  .landscape .art { aspect-ratio: 16 / 9; }

  .tile:hover .art,
  .tile:focus-visible .art {
    transform: scale(1.07);
    box-shadow: 0 18px 40px -12px oklch(0 0 0 / 0.8);
    z-index: 2;
  }
  .tile:focus-visible { outline: none; }
  .tile:focus-visible .art { outline: 2px solid var(--color-text); outline-offset: 2px; }

  .art-cover {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .art-blur {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    filter: blur(22px) saturate(1.4) brightness(0.55);
    transform: scale(1.3);
  }

  .art-fit {
    position: relative;
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
  }
  .art-fit.logo { padding: 14% 22%; filter: drop-shadow(0 4px 12px oklch(0 0 0 / 0.5)); }

  .art-empty {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: flex-end;
    padding: 14px;
    background:
      radial-gradient(120% 80% at 100% 0%, oklch(0.42 0.16 27 / 0.55), transparent 60%),
      var(--color-card);
  }
  .art-empty span {
    font-size: 0.95rem;
    font-weight: 800;
    line-height: 1.15;
    letter-spacing: -0.01em;
    color: var(--color-text);
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .live-pill {
    position: absolute;
    top: 8px;
    left: 8px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 7px;
    border-radius: 3px;
    background: var(--color-accent);
    color: var(--color-on-accent);
    font-size: 0.625rem;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .live-pill i {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: currentColor;
  }

  .hover-info {
    position: absolute;
    inset: auto 0 0 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 40px 10px 10px;
    background: linear-gradient(to top, oklch(0.1 0.004 20 / 0.95) 15%, transparent);
    opacity: 0;
    transition: opacity 200ms var(--ease-out);
  }
  .landscape .hover-info { background: linear-gradient(to top, oklch(0.1 0.004 20 / 0.7), transparent); }
  .tile:hover .hover-info,
  .tile:focus-visible .hover-info { opacity: 1; }

  .play-dot {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--color-text);
    color: var(--color-base);
    margin-bottom: 4px;
  }
  .play-dot :global(svg) { width: 14px; height: 14px; margin-left: 2px; }

  .hover-title {
    font-size: 0.8125rem;
    font-weight: 700;
    line-height: 1.2;
    color: var(--color-text);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .hover-sub,
  .caption-sub {
    font-size: 0.6875rem;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .caption {
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--color-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-bottom: -6px;
  }

  .pager {
    position: absolute;
    top: 18px;
    bottom: 22px;
    z-index: 5;
    width: var(--gutter);
    display: grid;
    place-items: center;
    padding: 0;
    color: var(--color-text);
    background: oklch(0.12 0.004 20 / 0.6);
    opacity: 0;
    transition: opacity 200ms var(--ease-out), background 200ms var(--ease-out);
  }
  .landscape ~ .pager,
  .pager:has(~ .landscape) { bottom: 62px; }
  .pager.prev { left: 0; border-radius: 0 6px 6px 0; }
  .pager.next { right: 0; border-radius: 6px 0 0 6px; }
  .pager :global(svg) { width: 28px; height: 28px; transition: transform 200ms var(--ease-out); }
  .row-body:hover .pager,
  .pager:focus-visible { opacity: 1; }
  .pager:hover { background: oklch(0.12 0.004 20 / 0.85); }
  .pager:hover :global(svg) { transform: scale(1.2); }

  @media (prefers-reduced-motion: reduce) {
    .art, .hover-info, .pager, .row-more { transition: none; }
    .tile:hover .art { transform: none; }
  }
</style>
