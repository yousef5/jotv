<script lang="ts" module>
  // Kept across navigation: back from the player lands on the same view
  type View = { kind: 'all' | 'fav' | 'recent' | 'uncat' } | { kind: 'cat'; id: number };
  type Sort = 'new' | 'old' | 'name' | 'long' | 'short' | 'plays';
  type Layout = 'mosaic' | 'reels';
  const ui = {
    view: { kind: 'all' } as View,
    query: '',
    tags: [] as string[],
    sort: 'new' as Sort,
    layout: 'mosaic' as Layout,
    kind: 'all' as 'all' | 'video' | 'image',
    collapsed: [] as number[],
  };
</script>

<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { fade, fly, slide } from 'svelte/transition';
  import { goto } from '$app/navigation';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import {
    reelUpdate, reelsMove, reelsTag, reelsDelete, reelsImport, reelCategorySave, reelCategoryDelete, showInFolder, reelSetCover,
  } from '$lib/tauri';
  import type { Reel, ReelCategory } from '$lib/tauri';
  import Dropdown from '$lib/components/Dropdown.svelte';
  import LinkAdd from '$lib/components/LinkAdd.svelte';
  import {
    reels, reelCats, reelsLoaded, reelQueue, loadReels, patchReel, thumbSrc, aspect, isPortrait, isAudio,
    childrenOf, familyIds, categoryPath, categoryColor, categoryOptions, linkDownloads, retryLink, dismissLink, fmtDuration, fmtSize, PLATFORM_COLORS, CATEGORY_COLORS,
  } from '$lib/stores/reels';


  let view = $state<View>(ui.view);
  let query = $state(ui.query);
  let activeTags = $state<string[]>(ui.tags);
  let sort = $state<Sort>(ui.sort);
  let layout = $state<Layout>(ui.layout);
  let kindFilter = $state<'all' | 'video' | 'image'>(ui.kind);
  let linkOpen = $state(false);
  let linkUrl = $state('');
  let collapsed = $state<Set<number>>(new Set(ui.collapsed));

  let selecting = $state(false);
  let selected = $state<Set<string>>(new Set());
  let editingId = $state<string | null>(null);
  let broken = $state<Set<string>>(new Set());
  let searchEl = $state<HTMLInputElement | undefined>();
  let dropping = $state(false);
  let importing = $state(false);
  let toast = $state('');
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  // Category editing in the rail
  type Draft = { id: number | null; parent: number | null; name: string; color: string | null };
  let draft = $state<Draft | null>(null);
  let confirmDeleteCat = $state<number | null>(null);
  let draftEl = $state<HTMLInputElement | undefined>();

  // Bulk bar
  let bulkTags = $state('');

  /** Videos waiting for "delete file / only remove" (one card, or the selection) */
  let askDelete = $state<string[] | null>(null);

  $effect(() => {
    ui.view = view;
    ui.query = query;
    ui.tags = activeTags;
    ui.sort = sort;
    ui.layout = layout;
    ui.kind = kindFilter;
    ui.collapsed = [...collapsed];
  });

  onMount(() => {
    if (!$reelsLoaded) loadReels().catch((e) => flash(String(e)));
    else loadReels().catch(() => {});

    // Drop video files or folders anywhere on the library to add them
    let unlisten: (() => void) | undefined;
    import('@tauri-apps/api/webview')
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((e) => {
          const p = e.payload;
          if (p.type === 'enter' || p.type === 'over') dropping = true;
          else if (p.type === 'leave') dropping = false;
          else if (p.type === 'drop') {
            dropping = false;
            if (p.paths.length) importPaths(p.paths);
          }
        }),
      )
      .then((u) => (unlisten = u))
      .catch(() => {});
    return () => unlisten?.();
  });

  function flash(msg: string) {
    toast = msg;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ''), 3200);
  }

  // ── Derived library views ─────────────────────────────────────────────────
  let mains = $derived($reelCats.filter((c) => c.parent_id === null));
  let catOptions = $derived(categoryOptions($reelCats));
  let sortOptions = $derived([
    { value: 'new', label: view.kind === 'recent' ? 'Last played' : 'Newest first' },
    { value: 'old', label: 'Oldest first' },
    { value: 'name', label: 'Name' },
    { value: 'plays', label: 'Most played' },
    { value: 'long', label: 'Longest' },
    { value: 'short', label: 'Shortest' },
  ]);
  let counts = $derived.by(() => {
    const direct = new Map<number, number>();
    for (const r of $reels) if (r.category_id !== null) direct.set(r.category_id, (direct.get(r.category_id) ?? 0) + 1);
    const total = new Map<number, number>();
    for (const c of $reelCats) {
      let n = direct.get(c.id) ?? 0;
      if (c.parent_id === null) for (const s of childrenOf($reelCats, c.id)) n += direct.get(s.id) ?? 0;
      total.set(c.id, n);
    }
    return total;
  });
  let favCount = $derived($reels.filter((r) => r.favorite).length);
  let recentCount = $derived($reels.filter((r) => r.last_played_at).length);
  let uncatCount = $derived($reels.filter((r) => r.category_id === null || !$reelCats.some((c) => c.id === r.category_id)).length);
  let totalDuration = $derived($reels.reduce((s, r) => s + (r.duration ?? 0), 0));

  let photoCount = $derived($reels.filter((r) => r.kind === 'image').length);

  /** Videos in the current view, before search and tag filters */
  let scoped = $derived.by(() => byView(kindFilter === 'all' ? $reels : $reels.filter((r) => r.kind === kindFilter)));
  function byView(all: Reel[]): Reel[] {
    const v = view;
    if (v.kind === 'fav') return all.filter((r) => r.favorite);
    if (v.kind === 'recent') return all.filter((r) => r.last_played_at);
    if (v.kind === 'uncat') return all.filter((r) => r.category_id === null || !$reelCats.some((c) => c.id === r.category_id));
    if (v.kind === 'cat') {
      const ids = familyIds($reelCats, v.id);
      return all.filter((r) => r.category_id !== null && ids.has(r.category_id));
    }
    return all;
  }

  let tagCounts = $derived.by(() => {
    const m = new Map<string, { tag: string; n: number }>();
    for (const r of scoped)
      for (const t of r.tags) {
        const k = t.toLowerCase();
        const e = m.get(k);
        if (e) e.n++;
        else m.set(k, { tag: t, n: 1 });
      }
    return [...m.values()].sort((a, b) => b.n - a.n || a.tag.localeCompare(b.tag));
  });
  let allTags = $derived.by(() => {
    const m = new Map<string, string>();
    for (const r of $reels) for (const t of r.tags) if (!m.has(t.toLowerCase())) m.set(t.toLowerCase(), t);
    return [...m.values()].sort((a, b) => a.localeCompare(b));
  });

  function norm(s: string): string {
    return s
      .toLowerCase()
      .normalize('NFKD')
      .replace(/[ً-ٰٟ̀-ͯ]/g, '')
      .replace(/[أإآ]/g, 'ا')
      .replace(/ى/g, 'ي')
      .replace(/ة/g, 'ه');
  }

  let filtered = $derived.by(() => {
    const words = norm(query.trim()).split(/\s+/).filter(Boolean);
    const want = activeTags.map((t) => t.toLowerCase());
    const list = scoped.filter((r) => {
      if (want.length && !want.every((t) => r.tags.some((x) => x.toLowerCase() === t))) return false;
      if (!words.length) return true;
      const cat = categoryPath($reelCats, r.category_id).map((c) => c.name).join(' ');
      const hay = norm(`${r.title} ${r.original_title} ${r.tags.join(' ')} ${r.platform} ${cat}`);
      return words.every((w) => hay.includes(w));
    });
    const by: Record<Sort, (a: Reel, b: Reel) => number> = {
      new: (a, b) => b.created_at.localeCompare(a.created_at),
      old: (a, b) => a.created_at.localeCompare(b.created_at),
      name: (a, b) => a.title.localeCompare(b.title),
      long: (a, b) => (b.duration ?? 0) - (a.duration ?? 0),
      short: (a, b) => (a.duration ?? Infinity) - (b.duration ?? Infinity),
      plays: (a, b) => b.plays - a.plays || (b.last_played_at ?? '').localeCompare(a.last_played_at ?? ''),
    };
    const s = view.kind === 'recent' && sort === 'new'
      ? (a: Reel, b: Reel) => (b.last_played_at ?? '').localeCompare(a.last_played_at ?? '')
      : by[sort];
    return [...list].sort(s);
  });

  /** The home view: rows per category, like a streaming service */
  let shelvesMode = $derived(view.kind === 'all' && kindFilter === 'all' && !query.trim() && !activeTags.length && $reels.length > 0);
  let hero = $derived.by(() => {
    if (!shelvesMode) return null;
    const pick = [...$reels].sort((a, b) => b.created_at.localeCompare(a.created_at));
    return pick.find((r) => r.kind !== 'image' && thumbSrc(r) && !broken.has(r.id)) ?? pick[0] ?? null;
  });
  let shelves = $derived.by(() => {
    if (!shelvesMode) return [];
    const newest = [...$reels].sort((a, b) => b.created_at.localeCompare(a.created_at));
    const rows: { key: string; title: string; color?: string; items: Reel[]; go: View; count: number }[] = [];
    const played = [...$reels].filter((r) => r.last_played_at).sort((a, b) => (b.last_played_at ?? '').localeCompare(a.last_played_at ?? ''));
    if (played.length) rows.push({ key: 'recent', title: 'Keep watching', items: played.slice(0, 24), go: { kind: 'recent' }, count: played.length });
    rows.push({ key: 'new', title: 'Recently added', items: newest.slice(0, 24), go: { kind: 'all' }, count: newest.length });
    const favs = newest.filter((r) => r.favorite);
    if (favs.length) rows.push({ key: 'fav', title: 'Favorites', items: favs.slice(0, 24), go: { kind: 'fav' }, count: favs.length });
    for (const m of mains) {
      const ids = familyIds($reelCats, m.id);
      const items = newest.filter((r) => r.category_id !== null && ids.has(r.category_id));
      if (items.length)
        rows.push({ key: `c${m.id}`, title: m.name, color: categoryColor($reelCats, m.id), items: items.slice(0, 24), go: { kind: 'cat', id: m.id }, count: items.length });
    }
    const unc = newest.filter((r) => r.category_id === null || !$reelCats.some((c) => c.id === r.category_id));
    if (unc.length && unc.length !== newest.length)
      rows.push({ key: 'uncat', title: 'Uncategorized', items: unc.slice(0, 24), go: { kind: 'uncat' }, count: unc.length });
    return rows;
  });

  let currentCat = $derived.by(() => {
    const v = view;
    return v.kind === 'cat' ? $reelCats.find((c) => c.id === v.id) ?? null : null;
  });
  let currentPath = $derived(currentCat ? categoryPath($reelCats, currentCat.id) : []);
  let subCats = $derived(currentCat && currentCat.parent_id === null ? childrenOf($reelCats, currentCat.id) : []);
  let viewTitle = $derived(
    view.kind === 'fav' ? 'Favorites'
    : view.kind === 'recent' ? 'Recently played'
    : view.kind === 'uncat' ? 'Uncategorized'
    : currentCat ? currentCat.name
    : 'All videos',
  );
  let editing = $derived(editingId ? $reels.find((r) => r.id === editingId) ?? null : null);

  // A category that disappeared (deleted) falls back to everything
  $effect(() => {
    if (view.kind === 'cat' && $reelsLoaded && !currentCat) view = { kind: 'all' };
  });

  function go(v: View) {
    view = v;
    activeTags = [];
    document.querySelector('main')?.scrollTo({ top: 0, behavior: 'smooth' });
  }

  function isView(v: View): boolean {
    return view.kind === v.kind && (v.kind !== 'cat' || (view.kind === 'cat' && view.id === v.id));
  }

  function toggleTag(t: string) {
    const k = t.toLowerCase();
    activeTags = activeTags.some((x) => x.toLowerCase() === k) ? activeTags.filter((x) => x.toLowerCase() !== k) : [...activeTags, t];
  }

  // ── Playing ───────────────────────────────────────────────────────────────
  function play(r: Reel, list: Reel[] = filtered) {
    reelQueue.set(list.map((x) => x.id));
    goto(`/reel?id=${encodeURIComponent(r.id)}`);
  }

  function onCard(e: MouseEvent, r: Reel, list: Reel[]) {
    if (selecting || e.ctrlKey || e.metaKey || e.shiftKey) {
      e.preventDefault();
      selecting = true;
      toggleSelect(r.id);
      return;
    }
    play(r, list);
  }

  // ── Selection ─────────────────────────────────────────────────────────────
  function toggleSelect(id: string) {
    const s = new Set(selected);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    selected = s;
  }

  function endSelect() {
    selecting = false;
    selected = new Set();
    bulkTags = '';
  }

  async function bulkMove(value: string) {
    const ids = [...selected];
    const cat = value === '' ? null : Number(value);
    await reelsMove(ids, cat);
    reels.update((l) => l.map((r) => (selected.has(r.id) ? { ...r, category_id: cat } : r)));
    flash(`Moved ${ids.length} video${ids.length === 1 ? '' : 's'} to ${cat === null ? 'Uncategorized' : categoryPath($reelCats, cat).map((c) => c.name).join(' › ')}`);
  }

  async function bulkAddTags() {
    const tags = splitTags(bulkTags);
    if (!tags.length) return;
    await reelsTag([...selected], tags);
    await loadReels();
    flash(`Tagged ${selected.size} video${selected.size === 1 ? '' : 's'}`);
    bulkTags = '';
  }

  async function bulkFavorite() {
    const list = $reels.filter((r) => selected.has(r.id));
    const on = !list.every((r) => r.favorite);
    for (const r of list) patchReel(await reelUpdate(r.id, r.title === r.original_title ? null : r.title, r.category_id, r.tags, on));
  }

  async function deleteReels(ids: string[], files: boolean) {
    askDelete = null;
    try {
      await reelsDelete(ids, files);
    } catch (e) {
      flash(String(e));
      return;
    }
    const gone = new Set(ids);
    reels.update((l) => l.filter((r) => !gone.has(r.id)));
    if (editingId && gone.has(editingId)) editingId = null;
    if (selected.size) {
      selected = new Set([...selected].filter((id) => !gone.has(id)));
      if (!selected.size) endSelect();
    }
    const n = ids.length === 1 ? 'Video' : `${ids.length} videos`;
    flash(files ? `${n} deleted from disk` : `${n} removed from the library (files kept on disk)`);
  }

  // ── Import ────────────────────────────────────────────────────────────────
  async function pickImport() {
    try {
      const picked = await openDialog({
        multiple: true,
        title: 'Add videos and photos to the library',
        filters: [
          { name: 'Videos & photos', extensions: ['mp4', 'mkv', 'webm', 'mov', 'avi', 'm4v', 'ts', 'flv', '3gp', 'jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp', 'avif'] },
          { name: 'Videos', extensions: ['mp4', 'mkv', 'webm', 'mov', 'avi', 'm4v', 'ts', 'flv', '3gp'] },
          { name: 'Photos', extensions: ['jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp', 'avif'] },
        ],
      });
      const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
      if (paths.length) await importPaths(paths);
    } catch (e) {
      flash(String(e));
    }
  }

  async function pickFolder() {
    try {
      const picked = await openDialog({ directory: true, title: 'Add a folder of videos' });
      if (typeof picked === 'string') await importPaths([picked]);
    } catch (e) {
      flash(String(e));
    }
  }

  async function importPaths(paths: string[]) {
    importing = true;
    try {
      const cat = view.kind === 'cat' ? view.id : null;
      const n = await reelsImport(paths, cat);
      await loadReels();
      flash(n ? `Added ${n} file${n === 1 ? '' : 's'}${cat !== null ? ` to ${currentCat?.name}` : ''}` : 'Nothing new to add (already in the library, or not a video or photo)');
    } catch (e) {
      flash(String(e));
    } finally {
      importing = false;
    }
  }

  // ── Categories ────────────────────────────────────────────────────────────
  async function startDraft(d: Draft) {
    confirmDeleteCat = null;
    draft = d;
    await tick();
    draftEl?.focus();
    draftEl?.select();
  }

  async function saveDraft() {
    const d = draft;
    if (!d) return;
    if (!d.name.trim()) {
      draft = null;
      return;
    }
    try {
      const saved = await reelCategorySave(d.id, d.name, d.parent, d.color);
      reelCats.update((l) => (d.id === null ? [...l.filter((c) => c.id !== saved.id), saved] : l.map((c) => (c.id === saved.id ? saved : c))));
      if (d.parent !== null) {
        const s = new Set(collapsed);
        s.delete(d.parent);
        collapsed = s;
      }
      draft = null;
      if (d.id === null) go({ kind: 'cat', id: saved.id });
    } catch (e) {
      flash(String(e));
    }
  }

  function draftKey(e: KeyboardEvent) {
    if (e.key === 'Enter') saveDraft();
    else if (e.key === 'Escape') draft = null;
  }

  async function deleteCat(c: ReelCategory) {
    if (confirmDeleteCat !== c.id) {
      confirmDeleteCat = c.id;
      return;
    }
    await reelCategoryDelete(c.id);
    const gone = familyIds($reelCats, c.id);
    reelCats.update((l) => l.filter((x) => !gone.has(x.id)));
    reels.update((l) => l.map((r) => (r.category_id !== null && gone.has(r.category_id) ? { ...r, category_id: null } : r)));
    confirmDeleteCat = null;
    if (view.kind === 'cat' && gone.has(view.id)) go({ kind: 'all' });
    flash(`Deleted “${c.name}”; its videos are now uncategorized`);
  }

  function toggleCollapse(id: number) {
    const s = new Set(collapsed);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    collapsed = s;
  }

  // ── Editor ────────────────────────────────────────────────────────────────
  let nameDraft = $state('');
  let tagInput = $state('');
  let confirmFileDelete = $state(false);
  $effect(() => {
    // New video in the editor: fresh fields
    const r = editing;
    if (r && r.id !== lastEdited) {
      lastEdited = r.id;
      nameDraft = r.title;
      tagInput = '';
      confirmFileDelete = false;
    }
  });
  let lastEdited = '';

  let tagSuggestions = $derived.by(() => {
    if (!editing) return [];
    const have = new Set(editing.tags.map((t) => t.toLowerCase()));
    const q = norm(tagInput.trim().replace(/^#/, ''));
    return allTags.filter((t) => !have.has(t.toLowerCase()) && (!q || norm(t).includes(q))).slice(0, 8);
  });

  function splitTags(s: string): string[] {
    return s.split(/[,،\n]/).map((t) => t.trim().replace(/^#/, '')).filter(Boolean);
  }

  async function save(r: Reel, patch: Partial<Pick<Reel, 'title' | 'category_id' | 'tags' | 'favorite'>>) {
    const next = { ...r, ...patch };
    patchReel(next);
    try {
      const name = next.title.trim() && next.title.trim() !== r.original_title ? next.title.trim() : null;
      patchReel(await reelUpdate(r.id, name, next.category_id, next.tags, next.favorite));
    } catch (e) {
      patchReel(r);
      flash(String(e));
    }
  }

  function commitName() {
    if (editing && nameDraft.trim() !== editing.title) save(editing, { title: nameDraft.trim() || editing.original_title });
  }

  function addTags(raw: string) {
    if (!editing) return;
    const add = splitTags(raw);
    if (!add.length) return;
    const tags = [...editing.tags];
    for (const t of add) if (!tags.some((x) => x.toLowerCase() === t.toLowerCase())) tags.push(t);
    save(editing, { tags });
    tagInput = '';
  }

  function tagKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      addTags(tagInput);
    } else if (e.key === 'Backspace' && !tagInput && editing?.tags.length) {
      save(editing, { tags: editing.tags.slice(0, -1) });
    }
  }

  async function pickCover(r: Reel) {
    try {
      const picked = await openDialog({
        title: 'Choose a cover image',
        filters: [{ name: 'Images', extensions: ['jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp', 'avif'] }],
      });
      if (typeof picked !== 'string') return;
      patchReel(await reelSetCover(r.id, picked));
      broken = new Set([...broken].filter((x) => x !== r.id));
      flash('Cover updated');
    } catch (e) {
      flash(String(e));
    }
  }

  async function resetCover(r: Reel) {
    try {
      patchReel(await reelSetCover(r.id, null));
      flash('Cover reset to a frame from the video');
    } catch (e) {
      flash(String(e));
    }
  }

  function openLink(url = '') {
    linkUrl = url;
    linkOpen = false;
    tick().then(() => (linkOpen = true));
  }

  // Ctrl+V a link anywhere on the library to add it
  function onPaste(e: ClipboardEvent) {
    const t = e.target;
    if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) return;
    const text = e.clipboardData?.getData('text')?.trim() ?? '';
    if (/^https?:\/\/\S+$/i.test(text)) {
      e.preventDefault();
      openLink(text);
    }
  }

  function onKey(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement;
    if (e.key === '/' && !typing) {
      e.preventDefault();
      searchEl?.focus();
    } else if (e.key === 'Delete' && !typing) {
      if (selecting && selected.size) askDelete = [...selected];
      else if (editingId) askDelete = [editingId];
    } else if (e.key === 'Escape') {
      if (typing) (e.target as HTMLElement).blur();
      else if (askDelete) askDelete = null;
      else if (editingId) editingId = null;
      else if (selecting) endSelect();
    } else if ((e.ctrlKey || e.metaKey) && e.key === 'a' && selecting && !typing) {
      e.preventDefault();
      selected = new Set(filtered.map((r) => r.id));
    }
  }

  function ago(iso: string): string {
    const t = new Date(iso.replace(' ', 'T') + (iso.includes('Z') ? '' : 'Z')).getTime();
    const d = (Date.now() - t) / 1000;
    if (!isFinite(d)) return '';
    if (d < 3600) return `${Math.max(1, Math.round(d / 60))}m ago`;
    if (d < 86400) return `${Math.round(d / 3600)}h ago`;
    if (d < 86400 * 30) return `${Math.round(d / 86400)}d ago`;
    return new Date(t).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
  }

  // Shelf arrows: page through a row; each arrow hides at its end
  function scrollShelf(e: MouseEvent, dir: 1 | -1) {
    const row = (e.currentTarget as HTMLElement).parentElement?.querySelector<HTMLElement>('.shelf-row');
    row?.scrollBy({ left: dir * row.clientWidth * 0.85, behavior: 'smooth' });
  }

  function shelfEdges(row: HTMLElement) {
    const wrap = row.parentElement;
    if (!wrap) return;
    wrap.classList.toggle('at-start', row.scrollLeft < 8);
    wrap.classList.toggle('at-end', row.scrollLeft + row.clientWidth > row.scrollWidth - 8);
  }

  function shelfInit(row: HTMLElement) {
    const ro = new ResizeObserver(() => shelfEdges(row));
    ro.observe(row);
    return { destroy: () => ro.disconnect() };
  }

  function hours(secs: number): string {
    if (secs < 3600) return `${Math.round(secs / 60)} min`;
    return `${(secs / 3600).toFixed(secs < 36000 ? 1 : 0)} h`;
  }
</script>

<svelte:window onkeydown={onKey} onpaste={onPaste} />

{#snippet photoIcon()}
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4.5" width="18" height="15" rx="2.5"/><circle cx="9" cy="10" r="1.8"/><path d="M21 16l-5-5-8.5 8.5"/></svg>
{/snippet}

{#snippet trashIcon()}
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16"/><path d="M9.5 7V4.8a.8.8 0 01.8-.8h3.4a.8.8 0 01.8.8V7"/><path d="M6.5 7l.8 12.2A1.8 1.8 0 009.1 21h5.8a1.8 1.8 0 001.8-1.8L17.5 7"/><path d="M10 11v6M14 11v6"/></svg>
{/snippet}

{#snippet catDot(color: string)}
  <span class="dot" style:background={color}></span>
{/snippet}

{#snippet card(r: Reel, list: Reel[], fixed: boolean)}
  {@const src = broken.has(r.id) ? null : thumbSrc(r)}
  {@const a = layout === 'reels' && !fixed ? 9 / 16 : aspect(r)}
  {@const sel = selected.has(r.id)}
  {@const path = categoryPath($reelCats, r.category_id)}
  <article
    class="card"
    class:portrait={isPortrait(r)}
    class:sel
    class:editing={editingId === r.id}
    style:--a={a}
  >
    <button
      class="thumb"
      onclick={(e) => onCard(e, r, list)}
      aria-label={selecting ? `Select ${r.title}` : `${r.kind === 'image' ? 'View' : 'Play'} ${r.title}`}
      aria-pressed={selecting ? sel : undefined}
    >
      {#if src}
        <img src={src} alt="" loading="lazy" decoding="async" onerror={() => (broken = new Set(broken).add(r.id))} />
      {:else}
        <span class="ph" class:audio={isAudio(r)} style:--c={PLATFORM_COLORS[r.platform] ?? 'var(--color-accent)'}>
          {#if isAudio(r)}
            <span class="bars" aria-hidden="true">{#each [5, 9, 14, 8, 12, 6, 10, 15, 7, 11, 4, 9, 13, 6] as h, i (i)}<i style:height="{h * 3}px"></i>{/each}</span>
            <small>Audio only</small>
          {:else}
            <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.5v13a1 1 0 001.5.86l10.5-6.5a1 1 0 000-1.72L9.5 4.64A1 1 0 008 5.5z"/></svg>
          {/if}
        </span>
      {/if}
      <span class="shade"></span>
      <span class="plat" style:--c={PLATFORM_COLORS[r.platform] ?? 'var(--color-text-muted)'}>{r.platform}</span>
      {#if r.kind === 'image'}
        <span class="dur photo" aria-label="Photo">{@render photoIcon()}</span>
      {:else if r.duration}
        <span class="dur">{fmtDuration(r.duration)}</span>
      {/if}
      {#if !selecting}
        <span class="playbtn" aria-hidden="true">
          {#if r.kind === 'image'}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5"/></svg>
          {:else}
            <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.5v13a1 1 0 001.5.86l10.5-6.5a1 1 0 000-1.72L9.5 4.64A1 1 0 008 5.5z"/></svg>
          {/if}
        </span>
      {/if}
      {#if selecting}
        <span class="check" aria-hidden="true">
          {#if sel}<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>{/if}
        </span>
      {/if}
      <span class="hover-meta" aria-hidden="true">
        <b dir="auto">{r.title}</b>
        {#if r.tags.length}<i dir="auto">{r.tags.slice(0, 3).map((t) => `#${t}`).join('  ')}</i>{/if}
      </span>
    </button>
    {#if !selecting}
      <div class="tools">
        <button
          class="tool"
          class:on={r.favorite}
          onclick={() => save(r, { favorite: !r.favorite })}
          aria-label={r.favorite ? 'Remove from favorites' : 'Add to favorites'}
          title={r.favorite ? 'Favorite' : 'Add to favorites'}
        >
          <svg viewBox="0 0 24 24" fill={r.favorite ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2.2" stroke-linejoin="round"><path d="M12 20.5s-7.5-4.6-9.2-9.4C1.6 7.6 4 4.5 7.2 4.5c2 0 3.6 1.1 4.8 2.8 1.2-1.7 2.8-2.8 4.8-2.8 3.2 0 5.6 3.1 4.4 6.6-1.7 4.8-9.2 9.4-9.2 9.4z"/></svg>
        </button>
        <button class="tool" onclick={() => (editingId = editingId === r.id ? null : r.id)} aria-label="Edit details" title="Name, category & tags">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20h4L19 9l-4-4L4 16v4z"/><path d="M13.5 6.5l4 4"/></svg>
        </button>
        <button class="tool del" onclick={() => (askDelete = [r.id])} aria-label="Delete video" title="Delete · Del">
          {@render trashIcon()}
        </button>
      </div>
    {/if}
    {#if askDelete?.length === 1 && askDelete[0] === r.id}
      <div class="confirm" transition:fade={{ duration: 120 }} role="alertdialog" aria-label="Delete this {r.kind === 'image' ? 'photo' : 'video'}?">
        <b>Delete this {r.kind === 'image' ? 'photo' : 'video'}?</b>
        <button class="btn danger solid small" onclick={() => deleteReels([r.id], true)}>Delete file from disk</button>
        <button class="btn ghost small" onclick={() => deleteReels([r.id], false)}>Only remove from library</button>
        <button class="link" onclick={() => (askDelete = null)}>Cancel</button>
      </div>
    {/if}
    <div class="meta">
      <h3 dir="auto" title={r.title}>{r.title}</h3>
      <p>
        {#if path.length}
          <button class="cat-link" onclick={() => go({ kind: 'cat', id: path[path.length - 1].id })}>
            {@render catDot(categoryColor($reelCats, r.category_id))}{path.map((c) => c.name).join(' › ')}
          </button>
        {:else}
          <span class="muted">{ago(r.created_at)}</span>
        {/if}
      </p>
    </div>
  </article>
{/snippet}

<div class="lib" class:with-editor={!!editing}>
  <!-- ── Rail ── -->
  <nav class="rail" aria-label="Library">
    <div class="rail-scroll">
      <p class="rail-head">Library</p>
      <button class="ri" class:on={isView({ kind: 'all' })} onclick={() => go({ kind: 'all' })}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><rect x="3.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.5"/></svg>
        <span>All videos</span><em>{$reels.length}</em>
      </button>
      <button class="ri" class:on={isView({ kind: 'fav' })} onclick={() => go({ kind: 'fav' })}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M12 20.5s-7.5-4.6-9.2-9.4C1.6 7.6 4 4.5 7.2 4.5c2 0 3.6 1.1 4.8 2.8 1.2-1.7 2.8-2.8 4.8-2.8 3.2 0 5.6 3.1 4.4 6.6-1.7 4.8-9.2 9.4-9.2 9.4z"/></svg>
        <span>Favorites</span><em>{favCount}</em>
      </button>
      <button class="ri" class:on={isView({ kind: 'recent' })} onclick={() => go({ kind: 'recent' })}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/></svg>
        <span>Recently played</span><em>{recentCount}</em>
      </button>
      <button class="ri" class:on={isView({ kind: 'uncat' })} onclick={() => go({ kind: 'uncat' })}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M3.5 13.5l2.6-7.2A1.5 1.5 0 017.5 5.3h9a1.5 1.5 0 011.4 1l2.6 7.2v4.7a1.5 1.5 0 01-1.5 1.5h-14a1.5 1.5 0 01-1.5-1.5z"/><path d="M3.5 13.5h5l1 2h5l1-2h5"/></svg>
        <span>Uncategorized</span><em>{uncatCount}</em>
      </button>

      <div class="rail-head row">
        <p>Categories</p>
        <button class="mini" onclick={() => startDraft({ id: null, parent: null, name: '', color: null })} aria-label="New category" title="New category">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
        </button>
      </div>

      {#snippet draftRow(sub: boolean)}
        <div class="draft" class:sub transition:slide={{ duration: 160 }}>
          <input
            bind:this={draftEl}
            bind:value={draft!.name}
            onkeydown={draftKey}
            onblur={() => setTimeout(() => draft && document.activeElement?.closest('.draft') === null && saveDraft(), 120)}
            placeholder={sub ? 'Sub-category name' : 'Category name'}
            dir="auto"
            maxlength="40"
          />
          {#if !sub}
            <div class="swatches" role="radiogroup" aria-label="Colour">
              {#each CATEGORY_COLORS as c (c)}
                <button
                  class="sw"
                  class:on={(draft?.color ?? null) === c}
                  style:background={c}
                  onmousedown={(e) => e.preventDefault()}
                  onclick={() => draft && (draft = { ...draft, color: c })}
                  role="radio"
                  aria-checked={(draft?.color ?? null) === c}
                  aria-label="Colour"
                ></button>
              {/each}
            </div>
          {/if}
          <p class="draft-hint">Enter to save · Esc to cancel</p>
        </div>
      {/snippet}

      {#if draft && draft.id === null && draft.parent === null}
        {@render draftRow(false)}
      {/if}

      {#if !mains.length && !draft}
        <p class="rail-empty">Group videos into categories like <b>Sports › Football</b> or <b>Cooking › Desserts</b>.</p>
      {/if}

      {#each mains as m (m.id)}
        {@const subs = childrenOf($reelCats, m.id)}
        {@const open = !collapsed.has(m.id)}
        {#if draft?.id === m.id}
          {@render draftRow(false)}
        {:else}
          <div class="ci" class:on={isView({ kind: 'cat', id: m.id })}>
            <button class="twisty" class:open class:hidden={!subs.length} onclick={() => toggleCollapse(m.id)} aria-label={open ? 'Collapse' : 'Expand'} tabindex={subs.length ? 0 : -1}>
              <svg viewBox="0 0 24 24" fill="currentColor"><path d="M9 6.5v11l8-5.5z"/></svg>
            </button>
            <button class="ci-main" onclick={() => go({ kind: 'cat', id: m.id })}>
              {@render catDot(categoryColor($reelCats, m.id))}
              <span dir="auto">{m.name}</span>
              <em>{counts.get(m.id) ?? 0}</em>
            </button>
            <span class="ci-tools">
              {#if confirmDeleteCat === m.id}
                <button class="mini danger wide" onclick={() => deleteCat(m)} onblur={() => (confirmDeleteCat = null)}>Delete?</button>
              {:else}
                <button class="mini" onclick={() => startDraft({ id: null, parent: m.id, name: '', color: null })} aria-label="Add sub-category" title="Add sub-category">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
                </button>
                <button class="mini" onclick={() => startDraft({ id: m.id, parent: null, name: m.name, color: m.color })} aria-label="Rename" title="Rename & colour">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20h4L19 9l-4-4L4 16v4z"/></svg>
                </button>
                <button class="mini" onclick={() => deleteCat(m)} aria-label="Delete category" title="Delete">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/></svg>
                </button>
              {/if}
            </span>
          </div>
        {/if}
        {#if open}
          {#each subs as s (s.id)}
            {#if draft?.id === s.id}
              {@render draftRow(true)}
            {:else}
              <div class="ci sub" class:on={isView({ kind: 'cat', id: s.id })}>
                <button class="ci-main" onclick={() => go({ kind: 'cat', id: s.id })}>
                  <span dir="auto">{s.name}</span>
                  <em>{counts.get(s.id) ?? 0}</em>
                </button>
                <span class="ci-tools">
                  {#if confirmDeleteCat === s.id}
                    <button class="mini danger wide" onclick={() => deleteCat(s)} onblur={() => (confirmDeleteCat = null)}>Delete?</button>
                  {:else}
                    <button class="mini" onclick={() => startDraft({ id: s.id, parent: m.id, name: s.name, color: null })} aria-label="Rename" title="Rename">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20h4L19 9l-4-4L4 16v4z"/></svg>
                    </button>
                    <button class="mini" onclick={() => deleteCat(s)} aria-label="Delete sub-category" title="Delete">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/></svg>
                    </button>
                  {/if}
                </span>
              </div>
            {/if}
          {/each}
          {#if draft && draft.id === null && draft.parent === m.id}
            {@render draftRow(true)}
          {/if}
        {/if}
      {/each}
    </div>

    <div class="rail-foot">
      <span>{$reels.length} videos</span>
      {#if totalDuration > 0}<span>· {hours(totalDuration)}</span>{/if}
    </div>
  </nav>

  <!-- ── Content ── -->
  <section class="content">
    <div class="toolbar">
      <label class="search">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><line x1="20.5" y1="20.5" x2="16" y2="16"/></svg>
        <input bind:this={searchEl} bind:value={query} placeholder="Search names, tags, categories" aria-label="Search the library" dir="auto" />
        {#if query}
          <button class="clear" onclick={() => (query = '')} aria-label="Clear search">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><line x1="7" y1="7" x2="17" y2="17"/><line x1="17" y1="7" x2="7" y2="17"/></svg>
          </button>
        {:else}
          <kbd>/</kbd>
        {/if}
      </label>

      {#if photoCount}
        <div class="seg kinds" role="radiogroup" aria-label="Show">
          <button class:on={kindFilter === 'all'} onclick={() => (kindFilter = 'all')} role="radio" aria-checked={kindFilter === 'all'}>All</button>
          <button class:on={kindFilter === 'video'} onclick={() => (kindFilter = 'video')} role="radio" aria-checked={kindFilter === 'video'}>Videos</button>
          <button class:on={kindFilter === 'image'} onclick={() => (kindFilter = 'image')} role="radio" aria-checked={kindFilter === 'image'}>Photos</button>
        </div>
      {/if}

      <span class="grow"></span>

      {#if !shelvesMode}
        <Dropdown label="Sort" value={sort} options={sortOptions} onchange={(v) => (sort = v as Sort)} />
        <div class="seg" role="radiogroup" aria-label="Layout">
          <button class:on={layout === 'mosaic'} onclick={() => (layout = 'mosaic')} role="radio" aria-checked={layout === 'mosaic'} title="Mosaic: every video in its own shape">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><rect x="3" y="4" width="11" height="7" rx="1.5"/><rect x="16.5" y="4" width="4.5" height="7" rx="1.5"/><rect x="3" y="13.5" width="5" height="7" rx="1.5"/><rect x="10.5" y="13.5" width="10.5" height="7" rx="1.5"/></svg>
          </button>
          <button class:on={layout === 'reels'} onclick={() => (layout = 'reels')} role="radio" aria-checked={layout === 'reels'} title="Reels: tall cards">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><rect x="3" y="3.5" width="5" height="17" rx="1.5"/><rect x="9.5" y="3.5" width="5" height="17" rx="1.5"/><rect x="16" y="3.5" width="5" height="17" rx="1.5"/></svg>
          </button>
        </div>
      {/if}
      <button class="btn ghost" class:on={selecting} onclick={() => (selecting ? endSelect() : (selecting = true))}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="3.5" width="17" height="17" rx="4"/><polyline points="8 12 11 15 16 9"/></svg>
        {selecting ? 'Done' : 'Select'}
      </button>
      <button class="btn primary" class:on={linkOpen} onclick={() => (linkOpen ? (linkOpen = false) : openLink())} title="Download from YouTube, Instagram, TikTok, Facebook, X, Reddit… (or just press Ctrl+V)">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 007.5.5l3-3a5 5 0 00-7-7l-1.7 1.7"/><path d="M14 11a5 5 0 00-7.5-.5l-3 3a5 5 0 007 7l1.7-1.7"/></svg>
        Add from link
      </button>
      <div class="split">
        <button class="btn" onclick={pickImport} disabled={importing} title="Videos and photos from this computer">
          {#if importing}
            <span class="spin" aria-hidden="true"></span>
          {:else}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
          {/if}
          Add files
        </button>
        <button class="btn icon" onclick={pickFolder} disabled={importing} aria-label="Add a folder" title="Add a whole folder">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round"><path d="M3 7.5A1.5 1.5 0 014.5 6h4.4l2 2.2h8.6A1.5 1.5 0 0121 9.7v8.8a1.5 1.5 0 01-1.5 1.5h-15A1.5 1.5 0 013 18.5z"/></svg>
        </button>
      </div>
    </div>

    {#if linkOpen}
      <LinkAdd initialUrl={linkUrl} categoryId={view.kind === 'cat' ? view.id : null} onclose={() => (linkOpen = false)} />
    {/if}

    {#if $linkDownloads.length}
      <section class="dls" aria-label="Downloading" transition:slide={{ duration: 200 }}>
        {#each $linkDownloads as d (d.id)}
          <div class="dl" class:failed={d.status === 'failed'} transition:fade={{ duration: 150 }}>
            <span class="dl-shot">
              {#if d.thumbnail}<img src={d.thumbnail} alt="" referrerpolicy="no-referrer" />{/if}
              {#if d.status === 'downloading'}
                <svg class="ring" viewBox="0 0 36 36" aria-hidden="true"><circle cx="18" cy="18" r="15" /><circle class="fill" cx="18" cy="18" r="15" stroke-dasharray="{d.progress * 94.25} 94.25" /></svg>
              {/if}
            </span>
            <span class="dl-text">
              <b dir="auto">{d.title}</b>
              {#if d.status === 'failed'}
                <small class="bad" title={d.error}>Failed{d.error ? `: ${d.error}` : ''}</small>
              {:else}
                <small>
                  <span style:color={PLATFORM_COLORS[d.platform]}>{d.platform}</span>
                  · {d.progress > 0 ? `${Math.round(d.progress * 100)}%` : 'Starting…'}{d.speed ? ` · ${d.speed}` : ''}{d.eta ? ` · ${d.eta} left` : ''}
                </small>
                <span class="bar"><i style:width="{Math.max(2, d.progress * 100)}%"></i></span>
              {/if}
            </span>
            {#if d.status === 'failed'}
              <button class="btn ghost small" onclick={() => retryLink(d)}>Retry</button>
              <button class="mini" onclick={() => dismissLink(d.id)} aria-label="Dismiss">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/></svg>
              </button>
            {/if}
          </div>
        {/each}
      </section>
    {/if}

    {#if !$reelsLoaded}
      <div class="skeleton" aria-label="Loading">
        {#each Array(8) as _, i (i)}<span style:--a={i % 3 === 0 ? 0.5625 : 1.78}></span>{/each}
      </div>
    {:else if !$reels.length}
      <!-- First run -->
      <div class="empty" in:fade={{ duration: 200 }}>
        <div class="empty-art" aria-hidden="true">
          <span></span><span></span><span></span>
        </div>
        <h2>Your video library starts here</h2>
        <p>Paste a link from YouTube, Instagram, TikTok, Facebook, X or Reddit, or add videos and photos you already have. You can also drop files and folders anywhere on this page.</p>
        <div class="empty-actions">
          <button class="btn primary" onclick={() => openLink()}>Add from link</button>
          <button class="btn" onclick={pickImport}>Add files</button>
          <button class="btn ghost" onclick={pickFolder}>Add a folder</button>
        </div>
      </div>
    {:else if shelvesMode}
      <!-- Home: featured + rows -->
      {#if hero}
        {@const src = broken.has(hero.id) ? null : thumbSrc(hero)}
        {@const hp = categoryPath($reelCats, hero.category_id)}
        <header class="hero" in:fade={{ duration: 250 }}>
          {#if src}<img class="hero-bg" src={src} alt="" aria-hidden="true" />{/if}
          <div class="hero-shot" style:--a={aspect(hero)}>
            {#if src}<img src={src} alt="" />{/if}
          </div>
          <div class="hero-text">
            <p class="eyebrow">
              <span class="newpill">Newest</span>
              {#if hp.length}
                <button class="cat-link" onclick={() => go({ kind: 'cat', id: hp[hp.length - 1].id })}>
                  {@render catDot(categoryColor($reelCats, hero.category_id))}{hp.map((c) => c.name).join(' › ')}
                </button>
              {/if}
            </p>
            <h2 dir="auto">{hero.title}</h2>
            <p class="hero-facts">
              <span style:color={PLATFORM_COLORS[hero.platform]}>{hero.platform}</span>
              {#if hero.duration}<span>{fmtDuration(hero.duration)}</span>{/if}
              {#if hero.height}<span>{hero.height >= 2160 ? '4K' : hero.height >= 1080 ? 'HD' : `${Math.min(hero.width ?? 0, hero.height)}p`}</span>{/if}
              <span>{ago(hero.created_at)}</span>
            </p>
            {#if hero.tags.length}
              <p class="hero-tags">
                {#each hero.tags.slice(0, 6) as t (t)}<button class="tag" onclick={() => toggleTag(t)}>#{t}</button>{/each}
              </p>
            {/if}
            <div class="hero-actions">
              <button class="btn play" onclick={() => play(hero!, [...$reels].sort((a, b) => b.created_at.localeCompare(a.created_at)))}>
                <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.5v13a1 1 0 001.5.86l10.5-6.5a1 1 0 000-1.72L9.5 4.64A1 1 0 008 5.5z"/></svg>
                {hero.kind === 'image' ? 'View' : 'Play'}
              </button>
              <button class="btn ghost" onclick={() => (editingId = hero!.id)}>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20h4L19 9l-4-4L4 16v4z"/></svg>
                Edit details
              </button>
            </div>
          </div>
        </header>
      {/if}

      {#if tagCounts.length}
        <div class="tagbar" aria-label="Popular tags">
          {#each tagCounts.slice(0, 20) as t (t.tag)}
            <button class="tag" onclick={() => toggleTag(t.tag)}>#{t.tag} <em>{t.n}</em></button>
          {/each}
        </div>
      {/if}

      {#each shelves as row (row.key)}
        <section class="shelf">
          <div class="shelf-head">
            <h2>
              {#if row.color}{@render catDot(row.color)}{/if}
              <span dir="auto">{row.title}</span>
              <em>{row.count}</em>
            </h2>
            <button class="see-all" onclick={() => go(row.go)}>
              See all
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 6 15 12 9 18"/></svg>
            </button>
          </div>
          <div class="shelf-wrap">
            <button class="arrow left" onclick={(e) => scrollShelf(e, -1)} aria-label="Scroll left" tabindex="-1">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 6 9 12 15 18"/></svg>
            </button>
            <div class="shelf-row" onscroll={(e) => shelfEdges(e.currentTarget as HTMLElement)} use:shelfInit>
              {#each row.items as r (r.id)}
                {@render card(r, row.items, true)}
              {/each}
            </div>
            <button class="arrow right" onclick={(e) => scrollShelf(e, 1)} aria-label="Scroll right" tabindex="-1">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 6 15 12 9 18"/></svg>
            </button>
          </div>
        </section>
      {/each}
    {:else}
      <!-- A category, a smart list, or search results -->
      <header class="view-head">
        {#if currentPath.length > 1}
          <nav class="crumbs" aria-label="Category path">
            <button onclick={() => go({ kind: 'cat', id: currentPath[0].id })}>{currentPath[0].name}</button>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 6 15 12 9 18"/></svg>
          </nav>
        {/if}
        <h1 dir="auto">
          {#if currentCat}{@render catDot(categoryColor($reelCats, currentCat.id))}{/if}
          {query.trim() && view.kind === 'all' ? 'Search' : viewTitle}
        </h1>
        <p class="count">
          {filtered.length} video{filtered.length === 1 ? '' : 's'}
          {#if filtered.length}· {hours(filtered.reduce((s, r) => s + (r.duration ?? 0), 0))}{/if}
        </p>
        {#if filtered.length > 1}
          <button class="btn play small" onclick={() => play(filtered[0], filtered)}>
            <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.5v13a1 1 0 001.5.86l10.5-6.5a1 1 0 000-1.72L9.5 4.64A1 1 0 008 5.5z"/></svg>
            Play all
          </button>
        {/if}
      </header>

      {#if subCats.length}
        <div class="subchips">
          {#each subCats as s (s.id)}
            <button class="subchip" onclick={() => go({ kind: 'cat', id: s.id })}>
              <span dir="auto">{s.name}</span><em>{counts.get(s.id) ?? 0}</em>
            </button>
          {/each}
          <button class="subchip add" onclick={() => currentCat && startDraft({ id: null, parent: currentCat.id, name: '', color: null })}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
            Sub-category
          </button>
        </div>
      {/if}

      {#if tagCounts.length}
        <div class="tagbar" aria-label="Filter by tag">
          {#each activeTags as t (t)}
            <button class="tag on" onclick={() => toggleTag(t)} aria-pressed="true">
              #{t}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"><line x1="7" y1="7" x2="17" y2="17"/><line x1="17" y1="7" x2="7" y2="17"/></svg>
            </button>
          {/each}
          {#each tagCounts.filter((t) => !activeTags.some((a) => a.toLowerCase() === t.tag.toLowerCase())).slice(0, 24) as t (t.tag)}
            <button class="tag" onclick={() => toggleTag(t.tag)} aria-pressed="false">#{t.tag} <em>{t.n}</em></button>
          {/each}
        </div>
      {/if}

      {#if filtered.length}
        <div class="grid" class:reels={layout === 'reels'}>
          {#each filtered as r (r.id)}
            {@render card(r, filtered, false)}
          {/each}
        </div>
      {:else}
        <div class="empty small" in:fade={{ duration: 150 }}>
          {#if query.trim() || activeTags.length}
            <h2>No matches</h2>
            <p>Nothing here matches {query.trim() ? `“${query.trim()}”` : ''}{query.trim() && activeTags.length ? ' with ' : ''}{activeTags.map((t) => `#${t}`).join(' ')}.</p>
            <button class="btn ghost" onclick={() => { query = ''; activeTags = []; }}>Clear filters</button>
          {:else if view.kind === 'fav'}
            <h2>No favorites yet</h2>
            <p>Tap the heart on any video to keep it here.</p>
          {:else if view.kind === 'recent'}
            <h2>Nothing played yet</h2>
            <p>Videos you watch show up here.</p>
          {:else if view.kind === 'cat'}
            <h2>This category is empty</h2>
            <p>Select videos and use <b>Move to</b>, set a category in a video's details, or drop files here to add them straight into <b>{currentCat?.name}</b>.</p>
          {:else}
            <h2>All sorted</h2>
            <p>Every video has a category.</p>
          {/if}
        </div>
      {/if}
    {/if}
  </section>

  <!-- ── Details editor ── -->
  {#if editing}
    {@const r = editing}
    {@const src = broken.has(r.id) ? null : thumbSrc(r)}
    <aside class="editor" transition:fly={{ x: 24, duration: 200 }} aria-label="Video details">
      <div class="ed-scroll">
        <div class="ed-top">
          <p>Details</p>
          <button class="mini" onclick={() => (editingId = null)} aria-label="Close details">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/></svg>
          </button>
        </div>

        <button class="ed-shot" style:--a={aspect(r)} onclick={() => play(r)} aria-label={r.kind === 'image' ? 'View' : 'Play'}>
          {#if src}<img src={src} alt="" />{/if}
          <span class="playbtn show">
            {#if r.kind === 'image'}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5"/></svg>
            {:else}
              <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.5v13a1 1 0 001.5.86l10.5-6.5a1 1 0 000-1.72L9.5 4.64A1 1 0 008 5.5z"/></svg>
            {/if}
          </span>
          {#if r.duration}<span class="dur">{fmtDuration(r.duration)}</span>{/if}
        </button>
        {#if r.kind !== 'image'}
          <div class="cover-row">
            <button class="btn ghost small" onclick={() => pickCover(r)}>
              {@render photoIcon()}
              Change cover
            </button>
            {#if r.thumb_path?.includes('-cover-')}
              <button class="link" onclick={() => resetCover(r)}>Use a frame instead</button>
            {/if}
          </div>
        {/if}

        <label class="fld">
          <span>Name</span>
          <input bind:value={nameDraft} onblur={commitName} onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()} dir="auto" />
          {#if r.title !== r.original_title}
            <button class="restore" onclick={() => { nameDraft = r.original_title; save(r, { title: r.original_title }); }} title={r.original_title}>Use original title</button>
          {/if}
        </label>

        <div class="fld">
          <span>Category</span>
          <Dropdown full label="Category" value={r.category_id === null ? '' : String(r.category_id)} options={catOptions} onchange={(v) => save(r, { category_id: v === '' ? null : Number(v) })} />
        </div>

        <div class="fld">
          <span>Tags</span>
          <div class="tagbox">
            {#each r.tags as t (t)}
              <span class="tchip">
                #{t}
                <button onclick={() => save(r, { tags: r.tags.filter((x) => x !== t) })} aria-label="Remove tag {t}">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"><line x1="7" y1="7" x2="17" y2="17"/><line x1="17" y1="7" x2="7" y2="17"/></svg>
                </button>
              </span>
            {/each}
            <input bind:value={tagInput} onkeydown={tagKey} onblur={() => tagInput.trim() && addTags(tagInput)} placeholder={r.tags.length ? 'Add tag' : 'Add tags, Enter between'} dir="auto" aria-label="Add a tag" />
          </div>
          {#if tagSuggestions.length}
            <div class="suggest">
              {#each tagSuggestions as t (t)}
                <button onmousedown={(e) => e.preventDefault()} onclick={() => addTags(t)}>+ {t}</button>
              {/each}
            </div>
          {/if}
        </div>

        <button class="fav-row" class:on={r.favorite} onclick={() => save(r, { favorite: !r.favorite })}>
          <svg viewBox="0 0 24 24" fill={r.favorite ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2.2" stroke-linejoin="round"><path d="M12 20.5s-7.5-4.6-9.2-9.4C1.6 7.6 4 4.5 7.2 4.5c2 0 3.6 1.1 4.8 2.8 1.2-1.7 2.8-2.8 4.8-2.8 3.2 0 5.6 3.1 4.4 6.6-1.7 4.8-9.2 9.4-9.2 9.4z"/></svg>
          {r.favorite ? 'In favorites' : 'Add to favorites'}
        </button>

        <dl class="facts">
          <div><dt>{r.kind === 'image' ? 'Type' : 'Source'}</dt><dd style:color={PLATFORM_COLORS[r.platform]}>{r.kind === 'image' ? 'Photo' : r.platform}</dd></div>
          {#if r.duration}<div><dt>Length</dt><dd>{fmtDuration(r.duration)}</dd></div>{/if}
          {#if r.width && r.height}<div><dt>{r.kind === 'image' ? 'Pixels' : 'Size'}</dt><dd>{r.width}×{r.height}{r.height > r.width ? ' · vertical' : ''}</dd></div>{/if}
          {#if r.file_size}<div><dt>File</dt><dd>{fmtSize(r.file_size)}</dd></div>{/if}
          <div><dt>Added</dt><dd>{ago(r.created_at)}</dd></div>
          <div><dt>{r.kind === 'image' ? 'Viewed' : 'Played'}</dt><dd>{r.plays ? `${r.plays}× · ${ago(r.last_played_at ?? r.created_at)}` : 'Never'}</dd></div>
        </dl>
        <p class="path" title={r.file_path}>{r.file_path}</p>

        <div class="ed-actions">
          <button class="btn ghost" onclick={() => showInFolder(r.file_path).catch(() => {})}>Show in folder</button>
          {#if confirmFileDelete}
            <button class="btn danger solid" onclick={() => deleteReels([r.id], true)}>Yes, delete the file from disk</button>
            <button class="btn ghost" onclick={() => (confirmFileDelete = false)}>Cancel</button>
          {:else}
            <button class="btn danger" onclick={() => (confirmFileDelete = true)}>
              {@render trashIcon()}
              Delete {r.kind === 'image' ? 'photo' : 'video'}…
            </button>
            <button class="btn ghost" onclick={() => deleteReels([r.id], false)}>Only remove from library</button>
          {/if}
        </div>
      </div>
    </aside>
  {/if}
</div>

<!-- Bulk delete: what to do with the files -->
{#if askDelete && askDelete.length > 0 && (askDelete.length > 1 || selecting)}
  {@const n = askDelete.length}
  <div class="bulk confirm-bar" transition:fly={{ y: 24, duration: 200 }} role="alertdialog" aria-label="Delete videos?">
    <span class="bulk-n">Delete <b>{n}</b> video{n === 1 ? '' : 's'}?</span>
    <button class="btn danger solid small" onclick={() => deleteReels(askDelete!, true)}>Delete file{n === 1 ? '' : 's'} from disk</button>
    <button class="btn ghost small" onclick={() => deleteReels(askDelete!, false)}>Only remove from library</button>
    <button class="ib" onclick={() => (askDelete = null)} aria-label="Cancel" title="Cancel · Esc">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/></svg>
    </button>
  </div>
{:else if selecting}
  <div class="bulk" transition:fly={{ y: 24, duration: 200 }} role="toolbar" aria-label="Selected videos">
    <span class="bulk-n"><b>{selected.size}</b> selected</span>
    <button class="link" onclick={() => (selected = new Set(filtered.length ? filtered.map((r) => r.id) : $reels.map((r) => r.id)))}>Select all</button>
    <span class="sep"></span>
    <Dropdown up label="Move to category" buttonText="Move to…" options={catOptions} disabled={!selected.size} onchange={bulkMove} />
    <form class="bulk-tags" onsubmit={(e) => { e.preventDefault(); bulkAddTags(); }}>
      <input bind:value={bulkTags} placeholder="Add tags…" disabled={!selected.size} aria-label="Tags to add" dir="auto" list="reel-tags" />
      <datalist id="reel-tags">{#each allTags as t (t)}<option value={t}></option>{/each}</datalist>
    </form>
    <button class="ib" onclick={bulkFavorite} disabled={!selected.size} aria-label="Toggle favorite" title="Favorite">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round"><path d="M12 20.5s-7.5-4.6-9.2-9.4C1.6 7.6 4 4.5 7.2 4.5c2 0 3.6 1.1 4.8 2.8 1.2-1.7 2.8-2.8 4.8-2.8 3.2 0 5.6 3.1 4.4 6.6-1.7 4.8-9.2 9.4-9.2 9.4z"/></svg>
    </button>
    <button class="btn danger small" onclick={() => (askDelete = [...selected])} disabled={!selected.size}>
      {@render trashIcon()}
      Delete
    </button>
    <button class="ib" onclick={endSelect} aria-label="Done selecting" title="Done · Esc">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/></svg>
    </button>
  </div>
{/if}

{#if dropping}
  <div class="dropzone" transition:fade={{ duration: 120 }}>
    <div>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 15V3"/><polyline points="7 8 12 3 17 8"/><path d="M4 14v4.5A1.5 1.5 0 005.5 20h13a1.5 1.5 0 001.5-1.5V14"/></svg>
      <b>Drop to add to {currentCat ? currentCat.name : 'the library'}</b>
      <span>Videos, photos or whole folders</span>
    </div>
  </div>
{/if}

{#if toast}
  <div class="toast" role="status" transition:fly={{ y: 12, duration: 180 }}>{toast}</div>
{/if}

<style>
  /* Arabic names stay beside their icons and dots in this left-to-right layout */
  [dir="auto"] { text-align: left; }

  .lib {
    --rail: 248px;
    display: grid;
    grid-template-columns: var(--rail) minmax(0, 1fr);
    gap: 28px;
    align-items: start;
  }
  .lib.with-editor { grid-template-columns: var(--rail) minmax(0, 1fr) 320px; }

  /* ── Rail ── */
  .rail {
    position: sticky;
    top: 0;
    max-height: calc(100vh - 120px);
    display: flex;
    flex-direction: column;
  }
  .rail-scroll {
    overflow-y: auto;
    padding-right: 4px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    scrollbar-width: thin;
  }
  .rail-head {
    margin: 18px 0 6px 10px;
    font-size: 0.6875rem;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }
  .rail-head:first-child { margin-top: 0; }
  .rail-head.row { display: flex; align-items: center; justify-content: space-between; margin-right: 2px; }

  .ri, .ci-main {
    display: flex;
    align-items: center;
    gap: 11px;
    width: 100%;
    min-height: 38px;
    padding: 0 10px;
    border-radius: 7px;
    background: none;
    color: oklch(0.86 0.004 25);
    font-size: 0.875rem;
    font-weight: 600;
    text-align: start;
    transition: background 120ms var(--ease-out), color 120ms var(--ease-out);
  }
  .ri :global(svg) { width: 18px; height: 18px; flex-shrink: 0; color: var(--color-text-muted); }
  .ri span:not(.dot), .ci-main span:not(.dot) { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ri em, .ci-main em {
    font-style: normal;
    font-size: 0.75rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: var(--color-text-muted);
  }
  .ri:hover, .ci:hover .ci-main { background: oklch(1 0 0 / 0.05); color: var(--color-text); }
  .ri.on, .ci.on .ci-main { background: oklch(1 0 0 / 0.09); color: var(--color-text); }
  .ri.on :global(svg) { color: var(--color-accent-soft); }
  .ri:focus-visible, .ci-main:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }

  .ci { position: relative; display: flex; align-items: center; }
  .ci .ci-main { padding-left: 24px; }
  .ci.sub .ci-main { padding-left: 46px; min-height: 34px; font-size: 0.8125rem; font-weight: 500; }
  .ci.sub::before {
    content: '';
    position: absolute;
    left: 29px;
    top: 0;
    bottom: 0;
    width: 1px;
    background: oklch(1 0 0 / 0.1);
  }
  .twisty {
    position: absolute;
    left: 2px;
    z-index: 1;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 5px;
    background: none;
    color: var(--color-text-muted);
  }
  .twisty :global(svg) { width: 14px; height: 14px; transition: transform 150ms var(--ease-out); }
  .twisty.open :global(svg) { transform: rotate(90deg); }
  .twisty.hidden { visibility: hidden; }
  .twisty:hover { color: var(--color-text); }
  .ci-tools {
    position: absolute;
    right: 4px;
    display: none;
    gap: 2px;
    padding-left: 12px;
    background: linear-gradient(to right, transparent, var(--color-hover) 12px);
    border-radius: 0 7px 7px 0;
  }
  .ci:hover .ci-main em, .ci:focus-within .ci-main em { visibility: hidden; }
  .ci:hover .ci-tools, .ci:focus-within .ci-tools { display: flex; }

  .mini {
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border-radius: 6px;
    background: none;
    color: var(--color-text-muted);
    transition: background 120ms var(--ease-out), color 120ms var(--ease-out);
  }
  .mini :global(svg) { width: 15px; height: 15px; }
  .mini:hover { background: oklch(1 0 0 / 0.1); color: var(--color-text); }
  .mini:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }
  .mini.danger { color: var(--color-accent-soft); }
  .mini.wide { width: auto; padding: 0 9px; font-size: 0.75rem; font-weight: 800; background: oklch(0.58 0.225 27 / 0.2); }

  .dot { display: inline-block; flex: none; width: 9px; height: 9px; border-radius: 50%; }

  .draft {
    margin: 2px 0 6px;
    padding: 8px;
    border-radius: 8px;
    background: var(--color-surface);
    box-shadow: inset 0 0 0 1px var(--color-border);
  }
  .draft.sub { margin-left: 22px; }
  .draft input {
    width: 100%;
    height: 34px;
    padding: 0 10px;
    border-radius: 6px;
    border: none;
    background: var(--color-base);
    color: var(--color-text);
    font-size: 0.875rem;
    font-weight: 600;
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
  }
  .swatches { display: flex; gap: 6px; margin-top: 8px; }
  .sw { width: 20px; height: 20px; border-radius: 50%; transition: transform 120ms var(--ease-out); }
  .sw.on { box-shadow: 0 0 0 2px var(--color-surface), 0 0 0 4px var(--color-text); }
  .sw:hover { transform: scale(1.12); }
  .draft-hint { margin-top: 6px; font-size: 0.6875rem; color: var(--color-text-muted); }
  .rail-empty { margin: 2px 10px; font-size: 0.8125rem; line-height: 1.5; color: var(--color-text-muted); }
  .rail-empty b { color: var(--color-text); font-weight: 600; }
  .rail-foot {
    display: flex;
    gap: 6px;
    margin-top: 12px;
    padding: 12px 10px 0;
    border-top: 1px solid oklch(1 0 0 / 0.07);
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-text-muted);
  }

  /* ── Toolbar ── */
  .content { min-width: 0; }
  .toolbar {
    position: sticky;
    top: -24px;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 8px;
    margin: -24px -8px 18px;
    padding: 24px 8px 12px;
    background: linear-gradient(to bottom, var(--color-base) 78%, transparent);
  }
  .grow { flex: 1; }
  .search {
    flex: 0 1 420px;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 40px;
    padding: 0 12px;
    border-radius: 8px;
    background: var(--color-surface);
    box-shadow: inset 0 0 0 1px var(--color-border);
    transition: box-shadow 150ms var(--ease-out);
  }
  .search:focus-within { box-shadow: inset 0 0 0 2px var(--color-text-muted); }
  .search > :global(svg) { width: 18px; height: 18px; color: var(--color-text-muted); flex-shrink: 0; }
  .search input { flex: 1; min-width: 0; height: 100%; border: none; background: none; color: var(--color-text); font-size: 0.875rem; outline: none; }
  .search kbd {
    padding: 1px 7px;
    border-radius: 4px;
    background: oklch(1 0 0 / 0.08);
    font: inherit;
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--color-text-muted);
  }
  .clear { display: grid; place-items: center; width: 24px; height: 24px; border-radius: 50%; background: oklch(1 0 0 / 0.1); color: var(--color-text); }
  .clear :global(svg) { width: 12px; height: 12px; }

  .seg { display: flex; padding: 3px; border-radius: 8px; background: var(--color-surface); box-shadow: inset 0 0 0 1px var(--color-border); }
  .seg button { width: 36px; height: 34px; display: grid; place-items: center; border-radius: 6px; background: none; color: var(--color-text-muted); }
  .seg button :global(svg) { width: 18px; height: 18px; }
  .seg button.on { background: oklch(1 0 0 / 0.12); color: var(--color-text); }

  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 40px;
    padding: 0 16px;
    border-radius: 8px;
    background: var(--color-text);
    color: oklch(0.16 0.004 25);
    font-size: 0.875rem;
    font-weight: 700;
    white-space: nowrap;
    transition: background 120ms var(--ease-out), transform 120ms var(--ease-out), opacity 120ms;
  }
  .btn :global(svg) { width: 18px; height: 18px; }
  .btn:hover { background: oklch(0.86 0.004 25); }
  .btn:active { transform: scale(0.97); }
  .btn:disabled { opacity: 0.5; pointer-events: none; }
  .btn:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }
  .btn.ghost { background: oklch(1 0 0 / 0.08); color: var(--color-text); }
  .btn.ghost:hover { background: oklch(1 0 0 / 0.14); }
  .btn.ghost.on { background: oklch(1 0 0 / 0.18); }
  .btn.play { background: var(--color-text); }
  .btn.primary { background: var(--color-accent); color: var(--color-on-accent); }
  .btn.primary:hover, .btn.primary.on { background: var(--color-accent-hover); }
  .seg.kinds button { width: auto; padding: 0 12px; font-size: 0.8125rem; font-weight: 700; }
  .dur.photo { display: grid; place-items: center; padding: 3px 5px; }
  .dur.photo :global(svg) { width: 14px; height: 14px; }
  .cover-row { display: flex; align-items: center; gap: 12px; margin-top: -6px; }
  .cover-row .link { font-size: 0.75rem; }

  /* Downloads on their way into the library */
  .dls { display: flex; flex-direction: column; gap: 8px; margin-bottom: 22px; }
  .dl {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 12px 8px 8px;
    border-radius: 10px;
    background: var(--color-surface);
    box-shadow: inset 0 0 0 1px var(--color-border);
  }
  .dl.failed { box-shadow: inset 0 0 0 1px oklch(0.58 0.225 27 / 0.5); }
  .dl-shot { position: relative; flex: none; width: 96px; height: 54px; border-radius: 6px; overflow: hidden; background: oklch(0.14 0.004 25); }
  .dl-shot img { width: 100%; height: 100%; object-fit: cover; opacity: 0.55; }
  .ring { position: absolute; inset: 0; margin: auto; width: 30px; height: 30px; transform: rotate(-90deg); }
  .ring circle { fill: none; stroke: oklch(1 0 0 / 0.25); stroke-width: 3.5; }
  .ring .fill { stroke: var(--color-text); stroke-linecap: round; transition: stroke-dasharray 300ms var(--ease-out); }
  .dl-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .dl-text b { font-size: 0.875rem; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .dl-text small { font-size: 0.75rem; font-weight: 600; color: var(--color-text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .dl-text small.bad { color: var(--color-accent-soft); }
  .bar { height: 3px; border-radius: 2px; background: oklch(1 0 0 / 0.1); overflow: hidden; }
  .bar i { display: block; height: 100%; border-radius: 2px; background: var(--color-accent); transition: width 300ms var(--ease-out); }
  .btn.danger { background: oklch(0.58 0.225 27 / 0.16); color: var(--color-accent-soft); }
  .btn.danger:hover { background: var(--color-accent); color: var(--color-on-accent); }
  .btn.small { height: 34px; padding: 0 12px; font-size: 0.8125rem; }
  .btn.icon { width: 40px; padding: 0; }
  .split { display: flex; gap: 1px; }
  .split .btn:first-child { border-radius: 8px 0 0 8px; }
  .split .btn:last-child { border-radius: 0 8px 8px 0; }
  .spin {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2.5px solid oklch(0 0 0 / 0.2);
    border-top-color: currentColor;
    animation: spin 700ms linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* ── Hero ── */
  .hero {
    position: relative;
    display: flex;
    align-items: center;
    gap: 32px;
    min-height: 300px;
    margin-bottom: 26px;
    padding: 28px 32px;
    border-radius: 14px;
    overflow: hidden;
    background: oklch(0.14 0.004 25);
    isolation: isolate;
  }
  .hero-bg {
    position: absolute;
    inset: -40px;
    width: calc(100% + 80px);
    height: calc(100% + 80px);
    object-fit: cover;
    filter: blur(48px) saturate(1.4) brightness(0.5);
    z-index: -2;
  }
  .hero::after {
    content: '';
    position: absolute;
    inset: 0;
    z-index: -1;
    background: linear-gradient(100deg, oklch(0.12 0.004 25 / 0.2) 0%, oklch(0.12 0.004 25 / 0.75) 70%);
  }
  .hero-shot {
    flex-shrink: 0;
    height: 244px;
    aspect-ratio: var(--a);
    max-width: 46%;
    border-radius: 10px;
    overflow: hidden;
    background: oklch(0 0 0 / 0.3);
    box-shadow: 0 30px 60px -20px oklch(0 0 0 / 0.8), 0 0 0 1px oklch(1 0 0 / 0.08);
  }
  .hero-shot img { width: 100%; height: 100%; object-fit: cover; }
  .hero-text { min-width: 0; display: flex; flex-direction: column; gap: 10px; }
  .eyebrow { display: flex; align-items: center; gap: 12px; font-size: 0.8125rem; }
  .newpill {
    padding: 3px 9px;
    border-radius: 4px;
    background: var(--color-accent);
    color: var(--color-on-accent);
    font-size: 0.6875rem;
    font-weight: 800;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .hero h2 {
    font-size: clamp(1.5rem, 2.4vw, 2.25rem);
    font-weight: 800;
    line-height: 1.15;
    letter-spacing: -0.02em;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    max-width: 30ch;
  }
  .hero-facts { display: flex; flex-wrap: wrap; gap: 6px 14px; font-size: 0.875rem; font-weight: 700; color: oklch(0.85 0.004 25); }
  .hero-tags { display: flex; flex-wrap: wrap; gap: 6px; }
  .hero-actions { display: flex; gap: 10px; margin-top: 6px; }
  .hero-actions .btn { height: 44px; padding: 0 22px; font-size: 0.9375rem; }

  .cat-link {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    max-width: 100%;
    background: none;
    color: inherit;
    font: inherit;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cat-link:hover { color: var(--color-text); text-decoration: underline; text-underline-offset: 3px; }

  /* ── Tags ── */
  .tagbar { display: flex; flex-wrap: wrap; gap: 7px; margin-bottom: 22px; }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border-radius: 15px;
    background: oklch(1 0 0 / 0.07);
    color: oklch(0.88 0.004 25);
    font-size: 0.8125rem;
    font-weight: 600;
    transition: background 120ms var(--ease-out), color 120ms var(--ease-out);
  }
  .tag em { font-style: normal; font-size: 0.6875rem; font-weight: 700; color: var(--color-text-muted); }
  .tag:hover { background: oklch(1 0 0 / 0.13); color: var(--color-text); }
  .tag.on { background: var(--color-text); color: oklch(0.16 0.004 25); }
  .tag.on :global(svg) { width: 11px; height: 11px; }

  /* ── Shelves ── */
  .shelf { margin-bottom: 30px; }
  .shelf-head { display: flex; align-items: baseline; justify-content: space-between; margin-bottom: 12px; }
  .shelf-head h2 { display: flex; align-items: center; gap: 10px; font-size: 1.1875rem; font-weight: 800; letter-spacing: -0.01em; }
  .shelf-head h2 em, .view-head .count { font-style: normal; font-size: 0.8125rem; font-weight: 600; color: var(--color-text-muted); }
  .see-all {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 4px 6px 4px 10px;
    border-radius: 6px;
    background: none;
    color: var(--color-text-muted);
    font-size: 0.8125rem;
    font-weight: 700;
  }
  .see-all :global(svg) { width: 15px; height: 15px; transition: transform 150ms var(--ease-out); }
  .see-all:hover { color: var(--color-text); background: oklch(1 0 0 / 0.06); }
  .see-all:hover :global(svg) { transform: translateX(2px); }
  .shelf-wrap { position: relative; }
  .shelf-row {
    --h: 230px;
    display: flex;
    gap: 14px;
    overflow-x: auto;
    overscroll-behavior-x: contain;
    scroll-snap-type: x proximity;
    padding: 6px 4px 4px;
    margin: -6px -4px 0;
    scrollbar-width: none;
  }
  .shelf-row::-webkit-scrollbar { display: none; }
  .arrow {
    position: absolute;
    top: calc(var(--h, 230px) / 2 - 24px);
    z-index: 3;
    width: 44px;
    height: 48px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    background: oklch(0.14 0.004 25 / 0.88);
    color: var(--color-text);
    box-shadow: 0 10px 24px -8px oklch(0 0 0 / 0.8), 0 0 0 1px oklch(1 0 0 / 0.1);
    opacity: 0;
    transition: opacity 160ms var(--ease-out), background 120ms var(--ease-out);
  }
  .arrow :global(svg) { width: 22px; height: 22px; }
  .arrow.left { left: -8px; }
  .arrow.right { right: -8px; }
  .arrow:hover { background: oklch(0.24 0.005 25); }
  .shelf-wrap:hover .arrow { opacity: 1; }
  :global(.shelf-wrap.at-start) .arrow.left, :global(.shelf-wrap.at-end) .arrow.right { opacity: 0; pointer-events: none; }
  .shelf-row .card { flex: 0 0 auto; width: calc(var(--h) * var(--a)); scroll-snap-align: start; }

  /* ── Category view ── */
  .view-head { display: flex; align-items: baseline; flex-wrap: wrap; gap: 4px 14px; margin-bottom: 16px; }
  .view-head h1 { display: flex; align-items: center; gap: 12px; font-size: 1.75rem; font-weight: 800; letter-spacing: -0.02em; }
  .view-head h1 .dot { width: 12px; height: 12px; }
  .view-head .btn { align-self: center; margin-left: auto; }
  .crumbs { flex-basis: 100%; display: flex; align-items: center; gap: 4px; font-size: 0.8125rem; font-weight: 700; color: var(--color-text-muted); }
  .crumbs button { background: none; color: inherit; font: inherit; }
  .crumbs button:hover { color: var(--color-text); text-decoration: underline; text-underline-offset: 3px; }
  .crumbs :global(svg) { width: 13px; height: 13px; }
  .subchips { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 14px; }
  .subchip {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    height: 36px;
    padding: 0 14px;
    border-radius: 8px;
    background: var(--color-surface);
    box-shadow: inset 0 0 0 1px var(--color-border);
    color: var(--color-text);
    font-size: 0.875rem;
    font-weight: 700;
    transition: background 120ms var(--ease-out);
  }
  .subchip em { font-style: normal; font-size: 0.75rem; color: var(--color-text-muted); }
  .subchip:hover { background: var(--color-hover); }
  .subchip.add { background: none; box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.14); color: var(--color-text-muted); }
  .subchip.add :global(svg) { width: 15px; height: 15px; }
  .subchip.add:hover { color: var(--color-text); }

  /* Justified mosaic: every card keeps its own shape, rows fill the width */
  .grid {
    --h: 230px;
    display: flex;
    flex-wrap: wrap;
    gap: 22px 14px;
    padding-bottom: 96px;
  }
  .grid::after { content: ''; flex-grow: 999999; }
  .grid .card { flex: var(--a) 1 calc(var(--h) * var(--a)); max-width: calc(var(--h) * var(--a) * 1.7); min-width: 0; }
  .grid.reels { display: grid; grid-template-columns: repeat(auto-fill, minmax(170px, 1fr)); gap: 22px 14px; }
  .grid.reels::after { display: none; }
  .grid.reels .card { max-width: none; }

  /* ── Card ── */
  .card { position: relative; display: flex; flex-direction: column; gap: 9px; }
  .thumb {
    position: relative;
    display: block;
    width: 100%;
    aspect-ratio: var(--a);
    border-radius: 10px;
    overflow: hidden;
    background: oklch(0.22 0.005 25);
    box-shadow: 0 0 0 1px oklch(1 0 0 / 0.06);
    transition: transform 220ms var(--ease-out), box-shadow 220ms var(--ease-out);
  }
  .thumb img { width: 100%; height: 100%; object-fit: cover; transition: transform 400ms var(--ease-out); }
  .card:hover .thumb { transform: translateY(-3px); box-shadow: 0 18px 36px -14px oklch(0 0 0 / 0.75), 0 0 0 1px oklch(1 0 0 / 0.14); }
  .card:hover .thumb img { transform: scale(1.04); }
  .thumb:focus-visible { outline: 3px solid var(--color-text); outline-offset: 2px; }
  .card.sel .thumb { box-shadow: 0 0 0 3px var(--color-accent); transform: scale(0.97); }
  .card.editing .thumb { box-shadow: 0 0 0 2px var(--color-text); }

  .ph {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background:
      radial-gradient(circle at 30% 20%, color-mix(in oklch, var(--c) 35%, transparent), transparent 60%),
      oklch(0.2 0.005 25);
    color: color-mix(in oklch, var(--c) 70%, white);
  }
  .ph :global(svg) { width: 34px; height: 34px; opacity: 0.7; }
  .ph.audio { place-content: center; gap: 10px; }
  .bars { display: flex; align-items: center; gap: 4px; height: 48px; }
  .bars i { width: 4px; border-radius: 2px; background: currentColor; opacity: 0.75; }
  .ph small { font-size: 0.6875rem; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; opacity: 0.8; }
  .shade {
    position: absolute;
    inset: 0;
    background: linear-gradient(to top, oklch(0 0 0 / 0.75), transparent 45%), linear-gradient(to bottom, oklch(0 0 0 / 0.35), transparent 30%);
    opacity: 0.8;
    transition: opacity 200ms var(--ease-out);
  }
  .card:hover .shade { opacity: 1; }
  .plat {
    position: absolute;
    top: 8px;
    left: 8px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 7px 3px 6px;
    border-radius: 4px;
    background: oklch(0 0 0 / 0.55);
    color: oklch(0.95 0.003 25);
    font-size: 0.6875rem;
    font-weight: 700;
    line-height: 1.2;
  }
  .plat::before { content: ''; width: 6px; height: 6px; border-radius: 50%; background: var(--c); }
  .dur {
    position: absolute;
    right: 8px;
    bottom: 8px;
    padding: 2px 6px;
    border-radius: 4px;
    background: oklch(0 0 0 / 0.7);
    color: oklch(0.97 0.003 25);
    font-size: 0.75rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .playbtn {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 52px;
    height: 52px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(0.97 0.003 25 / 0.92);
    color: oklch(0.16 0.004 25);
    opacity: 0;
    transform: translate(-50%, -50%) scale(0.85);
    transition: opacity 180ms var(--ease-out), transform 180ms var(--ease-out);
    box-shadow: 0 10px 30px -6px oklch(0 0 0 / 0.6);
  }
  .playbtn :global(svg) { width: 22px; height: 22px; margin-left: 3px; }
  .card:hover .playbtn, .thumb:focus-visible .playbtn, .playbtn.show { opacity: 1; transform: translate(-50%, -50%) scale(1); }
  .hover-meta {
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: 10px;
    display: none;
    flex-direction: column;
    gap: 3px;
    text-align: start;
  }
  .hover-meta b { font-size: 0.8125rem; font-weight: 700; line-height: 1.3; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .hover-meta i { font-style: normal; font-size: 0.6875rem; font-weight: 600; color: oklch(0.85 0.004 25); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  /* Tall cards show their title on the picture, like reels apps */
  .card.portrait .hover-meta { display: flex; }
  .card.portrait .dur { top: 8px; right: 8px; bottom: auto; }
  /* Narrow cards: the platform shows as its colour dot only */
  .card.portrait .plat { font-size: 0; gap: 0; padding: 5px; }
  .card.portrait .meta h3 { display: none; }

  .check {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(0 0 0 / 0.45);
    box-shadow: inset 0 0 0 2px oklch(1 0 0 / 0.85);
    color: var(--color-on-accent);
  }
  .check :global(svg) { width: 15px; height: 15px; }
  .card.sel .check { background: var(--color-accent); box-shadow: none; }
  .card.portrait .thumb:has(.check) .dur { display: none; }

  .tools {
    position: absolute;
    top: 6px;
    right: 6px;
    display: flex;
    gap: 4px;
    opacity: 0;
    transition: opacity 150ms var(--ease-out);
  }
  .card:hover .tools, .card:focus-within .tools { opacity: 1; }
  .card.portrait .tools { top: 34px; flex-direction: column; }
  .tool {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(0 0 0 / 0.6);
    color: oklch(0.97 0.003 25);
    transition: background 120ms var(--ease-out), transform 120ms var(--ease-out);
  }
  .tool :global(svg) { width: 15px; height: 15px; }
  .tool:hover { background: oklch(0 0 0 / 0.85); transform: scale(1.08); }
  .tool:focus-visible { outline: 2px solid var(--color-text); }
  .tool.on { color: var(--color-accent-soft); }
  /* A favorite's heart stays visible */
  .card:has(.tool.on) .tools { opacity: 1; }
  .card:has(.tool.on):not(:hover):not(:focus-within) .tool:not(.on) { visibility: hidden; }

  .tool.del:hover { background: var(--color-accent); }
  .confirm {
    position: absolute;
    inset: 0 0 auto;
    aspect-ratio: var(--a);
    min-height: 150px;
    z-index: 4;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    justify-content: center;
    gap: 7px;
    padding: 12px;
    border-radius: 10px;
    background: oklch(0.13 0.004 25 / 0.94);
    box-shadow: 0 0 0 2px var(--color-accent);
    text-align: center;
  }
  .confirm b { font-size: 0.875rem; margin-bottom: 2px; }
  .confirm .btn { height: 32px; padding: 0 8px; font-size: 0.75rem; white-space: normal; line-height: 1.15; }
  .confirm .link { font-size: 0.75rem; }
  .btn.danger.solid { background: var(--color-accent); color: var(--color-on-accent); }
  .btn.danger.solid:hover { background: var(--color-accent-hover); }
  .confirm-bar { box-shadow: 0 24px 50px -16px oklch(0 0 0 / 0.85), 0 0 0 1px var(--color-accent); }

  .meta { min-width: 0; padding: 0 2px; }
  .meta h3 {
    font-size: 0.875rem;
    font-weight: 700;
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .meta p { margin-top: 3px; font-size: 0.75rem; color: var(--color-text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta .cat-link { font-weight: 600; }
  .meta .cat-link .dot { width: 7px; height: 7px; }
  .muted { color: var(--color-text-muted); }

  /* ── Empty & loading ── */
  .skeleton { --h: 210px; display: flex; flex-wrap: wrap; gap: 14px; }
  .skeleton span {
    height: var(--h);
    flex: var(--a) 1 calc(var(--h) * var(--a));
    border-radius: 10px;
    background: linear-gradient(100deg, oklch(0.2 0.005 25) 40%, oklch(0.25 0.005 25) 50%, oklch(0.2 0.005 25) 60%) 0 0 / 300% 100%;
    animation: shimmer 1.4s linear infinite;
  }
  @keyframes shimmer { to { background-position: -150% 0; } }
  .empty {
    max-width: 520px;
    margin: 8vh auto 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    text-align: center;
  }
  .empty h2 { font-size: 1.5rem; font-weight: 800; letter-spacing: -0.02em; }
  .empty p { font-size: 0.9375rem; line-height: 1.55; color: var(--color-text-muted); }
  .empty p b { color: var(--color-text); }
  .empty.small { margin-top: 6vh; }
  .empty.small h2 { font-size: 1.125rem; }
  .empty-actions { display: flex; flex-wrap: wrap; justify-content: center; gap: 8px; margin-top: 8px; }
  .empty-art { position: relative; width: 190px; height: 150px; margin-bottom: 10px; }
  .empty-art span {
    position: absolute;
    bottom: 0;
    width: 74px;
    height: 132px;
    border-radius: 10px;
    background: linear-gradient(160deg, oklch(0.32 0.03 27), oklch(0.2 0.01 25));
    box-shadow: 0 16px 30px -12px oklch(0 0 0 / 0.7), inset 0 0 0 1px oklch(1 0 0 / 0.08);
  }
  .empty-art span:nth-child(1) { left: 6px; transform: rotate(-10deg); }
  .empty-art span:nth-child(2) { left: 58px; bottom: 12px; z-index: 1; background: linear-gradient(160deg, var(--color-accent), oklch(0.36 0.14 27)); }
  .empty-art span:nth-child(3) { left: 110px; transform: rotate(10deg); }

  /* ── Editor ── */
  .editor { position: sticky; top: 0; max-height: calc(100vh - 48px); display: flex; }
  .ed-scroll {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
    border-radius: 12px;
    background: var(--color-surface);
    box-shadow: inset 0 0 0 1px var(--color-border);
    scrollbar-width: thin;
  }
  .ed-top { display: flex; align-items: center; justify-content: space-between; }
  .ed-top p { font-size: 0.6875rem; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; color: var(--color-text-muted); }
  .ed-shot {
    position: relative;
    align-self: center;
    width: 100%;
    max-height: 320px;
    aspect-ratio: var(--a);
    border-radius: 10px;
    overflow: hidden;
    background: oklch(0.14 0.004 25);
  }
  .ed-shot img { width: 100%; height: 100%; object-fit: cover; }
  .ed-shot:hover .playbtn { transform: translate(-50%, -50%) scale(1.06); }

  .fld { display: flex; flex-direction: column; gap: 6px; }
  .fld > span { font-size: 0.75rem; font-weight: 700; color: var(--color-text-muted); }
  .fld input {
    height: 38px;
    padding: 0 11px;
    border-radius: 7px;
    border: none;
    background: var(--color-base);
    box-shadow: inset 0 0 0 1px var(--color-border);
    color: var(--color-text);
    font-size: 0.875rem;
    font-weight: 600;
  }
  .fld input:focus { outline: none; box-shadow: inset 0 0 0 2px var(--color-text-muted); }
  .restore { align-self: flex-start; background: none; color: var(--color-text-muted); font-size: 0.75rem; font-weight: 600; text-decoration: underline; text-underline-offset: 3px; }
  .restore:hover { color: var(--color-text); }
  .tagbox {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 6px;
    border-radius: 7px;
    background: var(--color-base);
    box-shadow: inset 0 0 0 1px var(--color-border);
  }
  .tagbox:focus-within { box-shadow: inset 0 0 0 2px var(--color-text-muted); }
  .tagbox input { flex: 1; min-width: 90px; height: 28px; padding: 0 4px; box-shadow: none; background: none; }
  .tagbox input:focus { box-shadow: none; }
  .tchip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 28px;
    padding: 0 4px 0 10px;
    border-radius: 14px;
    background: oklch(1 0 0 / 0.1);
    font-size: 0.8125rem;
    font-weight: 600;
  }
  .tchip button { width: 20px; height: 20px; display: grid; place-items: center; border-radius: 50%; background: none; color: var(--color-text-muted); }
  .tchip button:hover { background: oklch(1 0 0 / 0.15); color: var(--color-text); }
  .tchip :global(svg) { width: 10px; height: 10px; }
  .suggest { display: flex; flex-wrap: wrap; gap: 5px; }
  .suggest button {
    height: 26px;
    padding: 0 9px;
    border-radius: 13px;
    background: none;
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.14);
    color: var(--color-text-muted);
    font-size: 0.75rem;
    font-weight: 600;
  }
  .suggest button:hover { color: var(--color-text); background: oklch(1 0 0 / 0.06); }
  .fav-row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 40px;
    padding: 0 12px;
    border-radius: 8px;
    background: oklch(1 0 0 / 0.06);
    color: var(--color-text);
    font-size: 0.875rem;
    font-weight: 700;
  }
  .fav-row :global(svg) { width: 18px; height: 18px; }
  .fav-row.on { color: var(--color-accent-soft); background: oklch(0.58 0.225 27 / 0.14); }
  .facts { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 12px; }
  .facts div { min-width: 0; }
  .facts dt { font-size: 0.6875rem; font-weight: 700; color: var(--color-text-muted); }
  .facts dd { font-size: 0.8125rem; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .path { font-size: 0.6875rem; line-height: 1.45; color: var(--color-text-muted); word-break: break-all; }
  .ed-actions { display: flex; flex-direction: column; gap: 6px; }
  .ed-actions .btn { height: 36px; font-size: 0.8125rem; }

  /* ── Bulk bar ── */
  .bulk {
    position: fixed;
    left: calc(72px + 50%);
    bottom: 22px;
    z-index: 40;
    transform: translateX(calc(-50% - 36px));
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 16px;
    border-radius: 12px;
    background: oklch(0.24 0.005 25);
    box-shadow: 0 24px 50px -16px oklch(0 0 0 / 0.85), 0 0 0 1px oklch(1 0 0 / 0.1);
  }
  .bulk-n { font-size: 0.875rem; color: var(--color-text-muted); white-space: nowrap; }
  .bulk-n b { color: var(--color-text); font-variant-numeric: tabular-nums; }
  .link { background: none; color: var(--color-text-muted); font-size: 0.8125rem; font-weight: 700; white-space: nowrap; }
  .link:hover { color: var(--color-text); }
  .sep { width: 1px; height: 24px; background: oklch(1 0 0 / 0.12); margin: 0 4px; }
  .bulk { --dd-h: 36px; --dd-bg: oklch(1 0 0 / 0.08); --dd-border: transparent; }
  .bulk-tags input {
    width: 150px;
    height: 36px;
    padding: 0 10px;
    border-radius: 8px;
    border: none;
    background: oklch(1 0 0 / 0.08);
    color: var(--color-text);
    font-size: 0.8125rem;
  }
  .ib { width: 36px; height: 36px; display: grid; place-items: center; border-radius: 8px; background: none; color: var(--color-text); }
  .ib :global(svg) { width: 18px; height: 18px; }
  .ib:hover { background: oklch(1 0 0 / 0.1); }
  .ib:disabled, .bulk-tags input:disabled { opacity: 0.45; pointer-events: none; }

  .dropzone {
    position: fixed;
    inset: 0 0 0 72px;
    z-index: 60;
    display: grid;
    place-items: center;
    padding: 24px;
    background: oklch(0.12 0.004 25 / 0.8);
  }
  .dropzone > div {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    border-radius: 18px;
    border: 2px dashed oklch(1 0 0 / 0.35);
    color: var(--color-text-muted);
  }
  .dropzone :global(svg) { width: 52px; height: 52px; color: var(--color-text); }
  .dropzone b { font-size: 1.375rem; color: var(--color-text); }

  .toast {
    position: fixed;
    left: calc(72px + 50%);
    transform: translateX(calc(-50% - 36px));
    bottom: 90px;
    z-index: 70;
    max-width: min(560px, 80vw);
    padding: 11px 16px;
    border-radius: 8px;
    background: var(--color-text);
    color: oklch(0.16 0.004 25);
    font-size: 0.875rem;
    font-weight: 700;
    box-shadow: 0 18px 40px -12px oklch(0 0 0 / 0.7);
  }

  @media (max-width: 1180px) {
    .lib.with-editor { grid-template-columns: var(--rail) minmax(0, 1fr); }
    .lib.with-editor .editor {
      position: fixed;
      top: 24px;
      right: 24px;
      bottom: 24px;
      width: 320px;
      max-height: none;
      z-index: 30;
      box-shadow: 0 30px 60px -10px oklch(0 0 0 / 0.8);
      border-radius: 12px;
    }
  }
  @media (max-width: 980px) {
    .lib { --rail: 200px; gap: 18px; }
    .hero-shot { display: none; }
  }

  @media (prefers-reduced-motion: reduce) {
    .thumb, .thumb img, .playbtn, .see-all :global(svg) { transition: none; }
    .card:hover .thumb { transform: none; }
    .card:hover .thumb img { transform: none; }
    .skeleton span, .spin { animation: none; }
  }
</style>
