<script lang="ts">
  import { onMount } from 'svelte';
  import { mpvTracks } from '$lib/tauri';
  import type { MpvTrack } from '$lib/tauri';
  import { nowPlaying, setTrack } from '$lib/stores/live';

  // Audio and subtitle choice for what's playing. Lives below the player
  // (never over it: the video is a native surface that would cover it).

  let tracks = $state<MpvTrack[]>([]);
  let loaded = $state(false);

  let audio = $derived(tracks.filter((t) => t.kind === 'audio'));
  let subs = $derived(tracks.filter((t) => t.kind === 'sub'));
  let subsOff = $derived(!subs.some((t) => t.selected));

  // ISO 639-2 codes (what MKV files carry) → the 2-letter codes Intl understands
  const ISO3: Record<string, string> = {
    ara: 'ar', eng: 'en', fre: 'fr', fra: 'fr', ger: 'de', deu: 'de', spa: 'es', ita: 'it',
    por: 'pt', rus: 'ru', tur: 'tr', hin: 'hi', urd: 'ur', jpn: 'ja', kor: 'ko', chi: 'zh',
    zho: 'zh', per: 'fa', fas: 'fa', dut: 'nl', nld: 'nl', pol: 'pl', swe: 'sv', heb: 'he',
    gre: 'el', ell: 'el', ind: 'id', may: 'ms', msa: 'ms', tha: 'th', vie: 'vi', kur: 'ku',
  };
  const names = (() => {
    try { return new Intl.DisplayNames(['en'], { type: 'language' }); } catch { return null; }
  })();

  function language(code: string | null): string {
    if (!code || code === 'und') return 'Unknown language';
    const short = ISO3[code.toLowerCase()] ?? code;
    try {
      return names?.of(short) ?? code;
    } catch {
      return code;
    }
  }

  function label(t: MpvTrack): string {
    const parts = [language(t.lang)];
    if (t.title && !parts[0].toLowerCase().includes(t.title.toLowerCase())) parts.push(t.title);
    return parts.join(' · ');
  }

  function detail(t: MpvTrack): string {
    const bits: string[] = [];
    if (t.codec) bits.push(t.codec.toUpperCase());
    if (t.channels) bits.push(t.channels === 2 ? 'Stereo' : t.channels === 6 ? '5.1' : t.channels === 8 ? '7.1' : `${t.channels} ch`);
    if (t.external) bits.push('External');
    return bits.join(' · ');
  }

  async function refresh() {
    tracks = await mpvTracks().catch(() => []);
    loaded = true;
  }

  async function pick(kind: 'audio' | 'sub', t: MpvTrack | null) {
    tracks = tracks.map((x) => (x.kind === kind ? { ...x, selected: !!t && x.id === t.id } : x));
    await setTrack(kind, t).catch(() => {});
    refresh();
  }

  onMount(() => {
    // Tracks appear once the file is opened; catch late ones
    const t = setTimeout(refresh, 2000);
    return () => clearTimeout(t);
  });

  // New file → new tracks
  $effect(() => {
    $nowPlaying?.url;
    refresh();
  });
</script>

<div class="tracks">
  {#if !loaded}
    <p class="note">Reading tracks…</p>
  {:else if !audio.length && !subs.length}
    <p class="note">This stream has a single audio track and no subtitles.</p>
  {:else}
    <section>
      <h4>Audio</h4>
      {#each audio as t (t.id)}
        <button class="opt" class:on={t.selected} onclick={() => pick('audio', t)} aria-pressed={t.selected}>
          <i aria-hidden="true"></i>
          <span class="opt-text"><b>{label(t)}</b>{#if detail(t)}<small>{detail(t)}</small>{/if}</span>
        </button>
      {:else}
        <p class="note">Default audio only</p>
      {/each}
    </section>
    <section>
      <h4>Subtitles</h4>
      <button class="opt" class:on={subsOff} onclick={() => pick('sub', null)} aria-pressed={subsOff}>
        <i aria-hidden="true"></i><span class="opt-text"><b>Off</b></span>
      </button>
      {#each subs as t (t.id)}
        <button class="opt" class:on={t.selected} onclick={() => pick('sub', t)} aria-pressed={t.selected}>
          <i aria-hidden="true"></i>
          <span class="opt-text"><b>{label(t)}</b>{#if detail(t)}<small>{detail(t)}</small>{/if}</span>
        </button>
      {/each}
    </section>
  {/if}
</div>

<style>
  .tracks {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 6px 28px;
    padding: 14px 6px 6px;
  }
  section { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  h4 {
    margin: 0 0 6px 10px;
    font-size: 0.6875rem;
    font-weight: 800;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }
  .note { grid-column: 1 / -1; padding: 4px 10px; font-size: 0.8125rem; color: var(--color-text-muted); }

  .opt {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: 8px;
    background: none;
    color: var(--color-text);
    text-align: start;
    transition: background 120ms var(--ease-out);
  }
  .opt:hover { background: oklch(1 0 0 / 0.06); }
  .opt:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }
  .opt i {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 2px oklch(1 0 0 / 0.35);
    transition: box-shadow 120ms var(--ease-out);
  }
  .opt.on i { box-shadow: inset 0 0 0 5px var(--color-accent); }
  .opt-text { display: flex; flex-direction: column; min-width: 0; }
  .opt-text b { font-size: 0.875rem; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .opt-text small { font-size: 0.75rem; color: var(--color-text-muted); }
</style>
