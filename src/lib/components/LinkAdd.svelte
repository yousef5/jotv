<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { fade, slide } from 'svelte/transition';
  import { getSocialVideoInfo, checkYtdlp, reelCategorySave } from '$lib/tauri';
  import type { SocialVideoInfo } from '$lib/tauri';
  import {
    reels, reelCats, categoryOptions, addFromLink, fmtDuration, cleanTitle, titleHashtags, PLATFORM_COLORS,
  } from '$lib/stores/reels';
  import Dropdown from '$lib/components/Dropdown.svelte';
  import TagInput from '$lib/components/TagInput.svelte';

  // Paste a link → preview → pick quality, category and tags → it downloads
  // straight into the library.

  let {
    initialUrl = '',
    categoryId = null,
    onclose,
  }: { initialUrl?: string; categoryId?: number | null; onclose: () => void } = $props();

  const SITES = ['YouTube', 'Instagram', 'TikTok', 'Facebook', 'X', 'Reddit', 'Vimeo', 'Twitch', '+1000 more'];

  // Seeded once from the props: the panel is remounted for each new link
  let url = $state(untrack(() => initialUrl));
  let fetching = $state(false);
  let error = $state('');
  let missingTool = $state(false);
  let info = $state<SocialVideoInfo | null>(null);
  let name = $state('');
  let format = $state('');
  let category = $state(untrack(() => (categoryId === null ? '' : String(categoryId))));
  let tags = $state<string[]>([]);
  let allTags = $derived.by(() => {
    const m = new Map<string, string>();
    for (const r of $reels) for (const t of r.tags) if (!m.has(t.toLowerCase())) m.set(t.toLowerCase(), t);
    return [...m.values()].sort((a, b) => a.localeCompare(b));
  });

  // New category right here
  let newCat = $state<{ name: string; parent: string } | null>(null);
  let newCatEl = $state<HTMLInputElement | undefined>();
  let newCatError = $state('');
  let parentOptions = $derived([
    { value: '', label: 'Main category' },
    ...$reelCats.filter((c) => c.parent_id === null).map((c) => ({ value: String(c.id), label: `Inside ${c.name}` })),
  ]);

  async function openNewCat() {
    // Default: a sub-category of the chosen main category
    const cur = $reelCats.find((c) => String(c.id) === category);
    const main = cur ? (cur.parent_id ?? cur.id) : null;
    newCat = { name: '', parent: main === null ? '' : String(main) };
    newCatError = '';
    await tick();
    newCatEl?.focus();
  }

  async function createCat() {
    if (!newCat || !newCat.name.trim()) return;
    try {
      const c = await reelCategorySave(null, newCat.name, newCat.parent === '' ? null : Number(newCat.parent), null);
      reelCats.update((l) => [...l.filter((x) => x.id !== c.id), c]);
      category = String(c.id);
      newCat = null;
    } catch (e) {
      newCatError = String(e);
    }
  }
  let inputEl = $state<HTMLInputElement | undefined>();
  let fetchedFor = '';

  let formatOptions = $derived((info?.formats ?? []).map((f) => ({ value: f.format_id, label: f.label })));

  onMount(() => {
    inputEl?.focus();
    checkYtdlp().catch(() => (missingTool = true));
    if (initialUrl) fetchInfo();
  });

  function looksLikeLink(s: string): boolean {
    return /^https?:\/\/\S+\.\S+/i.test(s.trim());
  }

  async function fetchInfo() {
    const u = url.trim();
    if (!u || u === fetchedFor) return;
    if (!looksLikeLink(u)) {
      error = 'That doesn’t look like a link. Copy the address of the video (it starts with https://).';
      return;
    }
    fetchedFor = u;
    fetching = true;
    error = '';
    info = null;
    try {
      const i = await getSocialVideoInfo(u);
      if (url.trim() !== u) return; // the link changed meanwhile
      info = i;
      name = cleanTitle(i.title, i.uploader);
      tags = titleHashtags(i.title);
      format = i.formats[0]?.format_id ?? 'bestvideo+bestaudio/best';
    } catch (e) {
      fetchedFor = '';
      const msg = String(e);
      error = /unsupported url/i.test(msg)
        ? 'This site isn’t supported, or the link doesn’t point to a video.'
        : /private|login|sign in|cookies/i.test(msg)
          ? 'This video is private or needs a login, so it can’t be downloaded.'
          : msg.replace(/^yt-dlp error:\s*/, '').replace(/^ERROR:\s*/, '').slice(0, 220);
    } finally {
      fetching = false;
    }
  }

  async function onPaste() {
    await tick();
    if (looksLikeLink(url)) fetchInfo();
  }

  function add() {
    if (!info) return;
    const label = info.formats.find((f) => f.format_id === format)?.label ?? null;
    addFromLink(
      { url: info.url, title: name.trim() || info.title, thumbnail: info.thumbnail, platform: info.platform },
      {
        url: info.url,
        format,
        label,
        categoryId: category === '' ? null : Number(category),
        tags,
      },
    );
    // Ready for the next link
    info = null;
    url = '';
    tags = [];
    fetchedFor = '';
    inputEl?.focus();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section class="panel" transition:slide={{ duration: 220 }} aria-label="Add from a link" onkeydown={onKey}>
  <div class="row">
    <label class="url" class:busy={fetching}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 007.5.5l3-3a5 5 0 00-7-7l-1.7 1.7"/><path d="M14 11a5 5 0 00-7.5-.5l-3 3a5 5 0 007 7l1.7-1.7"/></svg>
      <input
        bind:this={inputEl}
        bind:value={url}
        onpaste={onPaste}
        onkeydown={(e) => e.key === 'Enter' && (info ? add() : fetchInfo())}
        placeholder="Paste a video link from YouTube, Instagram, TikTok, Facebook, X, Reddit…"
        aria-label="Video link"
        spellcheck="false"
      />
      {#if fetching}<span class="spin" aria-label="Looking up the video"></span>{/if}
    </label>
    {#if !info}
      <button class="btn" onclick={fetchInfo} disabled={fetching || !url.trim() || missingTool}>Find video</button>
    {/if}
    <button class="ib" onclick={onclose} aria-label="Close" title="Close · Esc">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/></svg>
    </button>
  </div>

  {#if missingTool}
    <p class="note warn">Downloading needs <b>yt-dlp</b>. Install it (<code>sudo pacman -S yt-dlp</code> or <code>pip install yt-dlp</code>) and reopen this panel.</p>
  {:else if error}
    <p class="note warn" transition:fade={{ duration: 120 }}>{error}</p>
  {:else if !info && !fetching}
    <p class="sites">
      {#each SITES as s (s)}<span style:--c={PLATFORM_COLORS[s === 'X' ? 'Twitter/X' : s] ?? 'oklch(0.6 0.01 25)'}>{s}</span>{/each}
    </p>
  {/if}

  {#if fetching}
    <div class="preview skeleton" aria-hidden="true"><span class="shot"></span><span class="lines"><i></i><i></i><i></i></span></div>
  {:else if info}
    <div class="preview" in:fade={{ duration: 160 }}>
      <div class="shot">
        {#if info.thumbnail}<img src={info.thumbnail} alt="" referrerpolicy="no-referrer" />{/if}
        {#if info.duration}<span class="dur">{fmtDuration(info.duration)}</span>{/if}
      </div>
      <div class="form">
        <p class="src">
          <span class="plat" style:--c={PLATFORM_COLORS[info.platform] ?? 'var(--color-text-muted)'}>{info.platform}</span>
          {#if info.uploader}<span dir="auto">{info.uploader}</span>{/if}
        </p>
        <label class="fld wide">
          <span>Name</span>
          <input bind:value={name} dir="auto" />
        </label>
        <div class="fld">
          <span>Quality</span>
          <Dropdown full label="Quality" value={format} options={formatOptions} onchange={(v) => (format = v)} />
        </div>
        <div class="fld">
          <span class="lab">
            Category
            {#if !newCat}<button class="add-cat" onclick={openNewCat}>+ New</button>{/if}
          </span>
          {#if newCat}
            <div class="newcat" transition:fade={{ duration: 120 }}>
              <input
                bind:this={newCatEl}
                bind:value={newCat.name}
                onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); createCat(); } else if (e.key === 'Escape') { e.stopPropagation(); newCat = null; } }}
                placeholder="Category name"
                dir="auto"
                maxlength="40"
              />
              <Dropdown label="Where" value={newCat.parent} options={parentOptions} onchange={(v) => newCat && (newCat = { ...newCat, parent: v })} />
              <button class="btn small" onclick={createCat} disabled={!newCat.name.trim()}>Create</button>
              <button class="ib small" onclick={() => (newCat = null)} aria-label="Cancel">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/></svg>
              </button>
            </div>
            {#if newCatError}<small class="bad">{newCatError}</small>{/if}
          {:else}
            <Dropdown full label="Category" value={category} options={categoryOptions($reelCats)} onchange={(v) => (category = v)} />
          {/if}
        </div>
        <div class="fld wide">
          <span>Tags</span>
          <TagInput bind:tags suggestions={allTags} placeholder="Type a tag and press Enter" />
        </div>
        <div class="go">
          <button class="btn primary" onclick={add}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v11"/><polyline points="7 10 12 15 17 10"/><path d="M4 19.5h16"/></svg>
            Download to library
          </button>
          <button class="btn ghost" onclick={() => { info = null; url = ''; fetchedFor = ''; inputEl?.focus(); }}>Different link</button>
        </div>
      </div>
    </div>
  {/if}
</section>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 22px;
    padding: 16px;
    border-radius: 12px;
    background: var(--color-surface);
    box-shadow: inset 0 0 0 1px var(--color-border), 0 20px 40px -24px oklch(0 0 0 / 0.8);
  }
  .row { display: flex; align-items: center; gap: 8px; }
  .url {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 46px;
    padding: 0 14px;
    border-radius: 9px;
    background: var(--color-base);
    box-shadow: inset 0 0 0 1px var(--color-border);
  }
  .url:focus-within { box-shadow: inset 0 0 0 2px var(--color-text-muted); }
  .url > svg { flex: none; width: 19px; height: 19px; color: var(--color-text-muted); }
  .url input { flex: 1; min-width: 0; height: 100%; padding: 0; border: none; background: none; color: var(--color-text); font-size: 0.9375rem; outline: none; }
  .spin {
    flex: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2.5px solid oklch(1 0 0 / 0.15);
    border-top-color: var(--color-text);
    animation: spin 700ms linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 46px;
    padding: 0 18px;
    border-radius: 9px;
    background: var(--color-text);
    color: oklch(0.16 0.004 25);
    font-size: 0.875rem;
    font-weight: 700;
    white-space: nowrap;
  }
  .btn :global(svg) { width: 18px; height: 18px; }
  .btn:hover { background: oklch(0.86 0.004 25); }
  .btn:disabled { opacity: 0.45; pointer-events: none; }
  .btn.primary { background: var(--color-accent); color: var(--color-on-accent); }
  .btn.primary:hover { background: var(--color-accent-hover); }
  .btn.ghost { background: oklch(1 0 0 / 0.08); color: var(--color-text); }
  .btn.ghost:hover { background: oklch(1 0 0 / 0.14); }
  .btn:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }
  .ib { width: 40px; height: 40px; display: grid; place-items: center; border-radius: 8px; background: none; color: var(--color-text-muted); }
  .ib:hover { background: oklch(1 0 0 / 0.08); color: var(--color-text); }
  .ib :global(svg) { width: 18px; height: 18px; }

  .sites { display: flex; flex-wrap: wrap; gap: 6px 14px; padding: 0 4px; font-size: 0.75rem; font-weight: 600; color: var(--color-text-muted); }
  .sites span { display: inline-flex; align-items: center; gap: 6px; }
  .sites span::before { content: ''; width: 6px; height: 6px; border-radius: 50%; background: var(--c); }
  .note { padding: 10px 12px; border-radius: 8px; font-size: 0.8125rem; line-height: 1.5; }
  .note.warn { background: oklch(0.58 0.225 27 / 0.12); color: oklch(0.88 0.04 27); }
  .note code { padding: 1px 5px; border-radius: 4px; background: oklch(0 0 0 / 0.3); font-size: 0.75rem; }

  .preview { display: flex; gap: 18px; align-items: flex-start; }
  .shot {
    position: relative;
    flex: none;
    width: 280px;
    aspect-ratio: 16 / 9;
    border-radius: 10px;
    overflow: hidden;
    background: oklch(0.14 0.004 25);
  }
  .shot img { width: 100%; height: 100%; object-fit: cover; }
  .dur { position: absolute; right: 8px; bottom: 8px; padding: 2px 6px; border-radius: 4px; background: oklch(0 0 0 / 0.7); font-size: 0.75rem; font-weight: 700; }
  .form { flex: 1; min-width: 0; display: grid; grid-template-columns: 1fr 1fr; gap: 12px 14px; }
  .src { grid-column: 1 / -1; display: flex; gap: 12px; align-items: center; font-size: 0.8125rem; font-weight: 600; color: var(--color-text-muted); }
  .plat { display: inline-flex; align-items: center; gap: 6px; color: var(--color-text); font-weight: 700; }
  .plat::before { content: ''; width: 7px; height: 7px; border-radius: 50%; background: var(--c); }
  .fld { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .fld.wide { grid-column: 1 / -1; }
  .fld > span { font-size: 0.75rem; font-weight: 700; color: var(--color-text-muted); }
  .fld small { font-weight: 500; }
  .fld input { height: 40px; padding: 0 11px; border-radius: 8px; border: none; background: var(--color-base); box-shadow: inset 0 0 0 1px var(--color-border); color: var(--color-text); font-size: 0.875rem; font-weight: 600; }
  .fld input:focus { outline: none; box-shadow: inset 0 0 0 2px var(--color-text-muted); }
  .form :global(.trigger) { --dd-bg: var(--color-base); }
  .go { grid-column: 1 / -1; display: flex; gap: 8px; }
  .lab { display: flex; align-items: center; justify-content: space-between; }
  .add-cat { padding: 0 4px; background: none; color: var(--color-accent-soft); font-size: 0.75rem; font-weight: 800; }
  .add-cat:hover { color: var(--color-text); text-decoration: underline; text-underline-offset: 3px; }
  .newcat { display: flex; gap: 6px; align-items: center; }
  .newcat input { flex: 1; min-width: 0; height: 40px; padding: 0 11px; border-radius: 8px; border: none; background: var(--color-base); box-shadow: inset 0 0 0 2px var(--color-accent); color: var(--color-text); font-size: 0.875rem; font-weight: 600; outline: none; text-align: left; }
  .newcat :global(.dd) { flex: none; width: 150px; }
  .btn.small { height: 40px; padding: 0 14px; }
  .ib.small { width: 34px; height: 34px; }
  .bad { font-size: 0.75rem; color: var(--color-accent-soft); }
  [dir="auto"] { text-align: left; }
  .go .btn { height: 42px; }

  .skeleton .shot, .skeleton .lines i {
    background: linear-gradient(100deg, oklch(0.22 0.005 25) 40%, oklch(0.27 0.005 25) 50%, oklch(0.22 0.005 25) 60%) 0 0 / 300% 100%;
    animation: shimmer 1.3s linear infinite;
  }
  .skeleton .lines { flex: 1; display: flex; flex-direction: column; gap: 10px; padding-top: 6px; }
  .skeleton .lines i { height: 14px; border-radius: 4px; }
  .skeleton .lines i:nth-child(1) { width: 30%; }
  .skeleton .lines i:nth-child(2) { width: 85%; height: 20px; }
  .skeleton .lines i:nth-child(3) { width: 60%; }
  @keyframes shimmer { to { background-position: -150% 0; } }

  @media (max-width: 1100px) {
    .preview { flex-direction: column; }
    .shot { width: 100%; max-width: 360px; }
  }
  @media (prefers-reduced-motion: reduce) {
    .spin, .skeleton .shot, .skeleton .lines i { animation: none; }
  }
</style>
