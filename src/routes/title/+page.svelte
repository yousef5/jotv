<script lang="ts">
  import { askFavoriteCategory } from '$lib/components/FavoriteSaved.svelte';
  import { onDestroy } from 'svelte';
  import { fade } from 'svelte/transition';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import {
    getChannel, getVodInfo, getSeriesInfo, getChannelsByGroup, isFavorite, toggleFavorite, tmdbDetails,
    startDownload, getSetting,
  } from '$lib/tauri';
  import type { Channel, Playlist, SeasonDetail, EpisodeDetail, TmdbDetails } from '$lib/tauri';
  import { playlists, loadPlaylists, pendingOpen } from '$lib/stores/playlists';
  import { downloads } from '$lib/stores/downloads';
  import {
    playExternal, extFromUrl, splitTitle, firstUrl, toRating, formatRuntime, remoteIdFromUrl,
    backdropUrl, isWideBackdrop, searchTitle,
  } from '$lib/playback';
  import { pendingWatch, watchHref } from '$lib/stores/watch';
  import type { WatchItem } from '$lib/stores/watch';
  import { resumePosition } from '$lib/stores/live';
  import { mainScrollSnapshot } from '$lib/scroll';

  export const snapshot = mainScrollSnapshot;

  interface Meta {
    backdrop: string | null;
    cover: string | null;
    plot: string | null;
    genres: string[];
    cast: string | null;
    director: string | null;
    released: string | null;
    year: string | null;
    rating: number | null;
    runtime: string | null;
    trailer: string | null;
  }

  let ch = $state<Channel | null>(null);
  let meta = $state<Meta | null>(null);
  let seasons = $state<SeasonDetail[]>([]);
  let season = $state<string | null>(null);
  let related = $state<Channel[]>([]);
  let favorite = $state(false);
  let loading = $state(true);
  let metaLoading = $state(false);
  let error = $state('');
  let notice = $state('');
  let backdropReady = $state(false);
  let resume = $state<{ season: string; episode: string } | null>(null);

  /** Movies: where you stopped last time */
  let movieResume = $state<{ pos: number; duration: number } | null>(null);
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  let loadToken = 0;

  let id = $derived(parseInt($page.url.searchParams.get('id') ?? ''));
  let names = $derived(ch ? splitTitle(ch.name) : null);
  let isSeries = $derived(ch?.content_type === 'series');
  let episodes = $derived(seasons.find((s) => s.season_number === season)?.episodes ?? []);
  /** Optional TMDB artwork (when the user set a key) */
  let tmdb = $state<TmdbDetails | null>(null);
  let artwork = $derived(tmdb?.backdrop ?? meta?.backdrop ?? null);
  let plot = $derived(meta?.plot ?? tmdb?.overview ?? null);
  let trailer = $derived(meta?.trailer ?? tmdb?.trailer ?? null);
  let rating = $derived(meta?.rating ?? (tmdb?.rating ? Math.round(tmdb.rating * 10) / 10 : null));
  let poster = $derived(meta?.cover || ch?.logo_url || null);
  let vodDownload = $derived(ch && !isSeries ? activeDownload((d) => d.channel_id === ch!.id) : null);

  let resumeEpisode = $derived.by(() => {
    if (!seasons.length) return null;
    if (resume) {
      const s = seasons.find((x) => x.season_number === resume!.season);
      const e = s?.episodes.find((x) => x.episode_num === resume!.episode);
      if (s && e) return { season: s, episode: e, resumed: true };
    }
    const first = seasons[0];
    return first.episodes[0] ? { season: first, episode: first.episodes[0], resumed: false } : null;
  });

  $effect(() => {
    if (!isNaN(id)) load(id);
  });

  onDestroy(() => {
    clearTimeout(noticeTimer);
  });

  async function load(channelId: number) {
    const token = ++loadToken;
    movieResume = null;
    ch = null;
    meta = null;
    seasons = [];
    season = null;
    related = [];
    resume = null;
    error = '';
    loading = true;
    backdropReady = false;

    let channel: Channel;
    try {
      channel = await getChannel(channelId);
    } catch (e) {
      if (token === loadToken) { error = String(e); loading = false; }
      return;
    }
    if (token !== loadToken) return;

    if (channel.content_type === 'live') {
      pendingOpen.set({ playlistId: channel.playlist_id, contentType: 'live', channel, action: 'play' });
      goto('/live', { replaceState: true });
      return;
    }

    ch = channel;
    loading = false;
    if (!$playlists.length) await loadPlaylists();
    const playlist = $playlists.find((p) => p.id === channel.playlist_id);

    isFavorite(channel.id).then((f) => token === loadToken && (favorite = f)).catch(() => {});
    tmdb = null;
    const titleYear = splitTitle(channel.name).year;
    tmdbDetails(channel.content_type as 'vod' | 'series', searchTitle(channel.name), titleYear ? +titleYear : null)
      .then((d) => token === loadToken && (tmdb = d))
      .catch(() => {});
    if (channel.content_type === 'vod') {
      resumePosition(`progress_vod_${channel.id}`)
        .then((r) => token === loadToken && r.pos > 0 && (movieResume = r))
        .catch(() => {});
    }
    getChannelsByGroup(channel.playlist_id, channel.content_type, channel.group_name, 40, 0)
      .then((list) => token === loadToken && (related = list.filter((c) => c.id !== channel.id).slice(0, 24)))
      .catch(() => {});
    if (channel.content_type === 'series') {
      getSetting(`series_progress_${channel.id}`)
        .then((v) => {
          const [s, e] = (v ?? '').split(':');
          if (token === loadToken && s && e) resume = { season: s, episode: e };
        })
        .catch(() => {});
    }

    if (playlist) await loadMeta(channel, playlist, token);
  }

  async function loadMeta(channel: Channel, p: Playlist, token: number) {
    if (p.source_type !== 'xtream' || !p.source_url || !p.xtream_username || !p.xtream_password) return;
    const remoteId = remoteIdFromUrl(channel.stream_url);
    if (remoteId === null) return;
    metaLoading = true;
    try {
      if (channel.content_type === 'series') {
        const d = await getSeriesInfo(p.source_url, p.xtream_username, p.xtream_password, remoteId);
        if (token !== loadToken) return;
        meta = toMeta(d.info, null, null);
        seasons = d.seasons;
        season = resume?.season && d.seasons.some((s) => s.season_number === resume!.season)
          ? resume.season
          : d.seasons[0]?.season_number ?? null;
      } else {
        const d = await getVodInfo(p.source_url, p.xtream_username, p.xtream_password, remoteId);
        if (token !== loadToken) return;
        meta = toMeta(d.info, d.info.duration_secs, d.info.youtube_trailer);
      }
    } catch (e) {
      if (token === loadToken) flash(`Couldn't load details: ${e}`);
    } finally {
      if (token === loadToken) metaLoading = false;
    }
  }

  function toMeta(
    info: { cover: string | null; plot: string | null; genre: string | null; cast: string | null; director: string | null; release_date: string | null; rating: unknown; backdrop_path: unknown },
    durationSecs: number | null,
    trailer: string | null,
  ): Meta {
    return {
      backdrop: backdropUrl(info.backdrop_path, info.cover),
      cover: firstUrl(info.cover),
      plot: info.plot?.trim() || null,
      genres: (info.genre ?? '').split(/[,/]/).map((g) => g.trim()).filter(Boolean),
      cast: info.cast?.trim() || null,
      director: info.director?.trim() || null,
      released: info.release_date?.trim() || null,
      year: info.release_date?.slice(0, 4) || null,
      rating: toRating(info.rating),
      runtime: formatRuntime(durationSecs),
      trailer: trailer?.trim() || null,
    };
  }

  // ── Actions ─────────────────────────────────────────────────────────────────

  function flash(msg: string) {
    notice = msg;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ''), 4500);
  }

  // Movies and episodes play in the app's own player (/watch)
  function openWatch(items: WatchItem[], index: number) {
    if (!ch || !items[index]) return;
    pendingWatch.set({ channel: ch, poster, items, index });
    goto(watchHref(ch.id, items[index], isSeries));
  }

  function episodeItems(): WatchItem[] {
    const title = names?.title ?? '';
    return seasons.flatMap((s) =>
      s.episodes.map((e) => ({
        id: e.id,
        url: e.stream_url,
        title,
        subtitle: `S${s.season_number}:E${e.episode_num}`,
        progressKey: `progress_ep_${e.id}`,
        season: s.season_number,
        episode: e.episode_num,
      })),
    );
  }

  function playEpisode(s: SeasonDetail, e: EpisodeDetail) {
    if (!ch) return;
    resume = { season: s.season_number, episode: e.episode_num };
    const items = episodeItems();
    openWatch(items, items.findIndex((x) => x.id === e.id));
  }

  function playMain() {
    if (!ch) return;
    if (!isSeries) {
      return openWatch([{ id: ch.id, url: ch.stream_url, title: names!.title, progressKey: `progress_vod_${ch.id}` }], 0);
    }
    if (resumeEpisode) return playEpisode(resumeEpisode.season, resumeEpisode.episode);
    flash(metaLoading ? 'Episodes are still loading…' : 'No episodes available for this series.');
  }

  function fmtLeft(secs: number): string {
    const m = Math.max(1, Math.round(secs / 60));
    return m >= 60 ? `${Math.floor(m / 60)}h ${m % 60}m left` : `${m}m left`;
  }

  async function playTrailer() {
    if (!trailer) return;
    try {
      await playExternal(`https://www.youtube.com/watch?v=${trailer}`, true);
    } catch (e) {
      flash(e instanceof Error ? e.message : String(e));
    }
  }

  async function toggleList(e?: MouseEvent) {
    if (!ch) return;
    const anchor = e?.currentTarget as Element | undefined;
    try {
      favorite = await toggleFavorite(ch.id);
      if (favorite) askFavoriteCategory(ch, anchor);
      else flash('Removed from My list');
    } catch (e) {
      flash(String(e));
    }
  }

  function activeDownload(match: (d: { channel_id: number | null; url: string }) => boolean) {
    const d = $downloads.find((x) => match(x) && ['downloading', 'queued', 'paused'].includes(x.status));
    return d ? { progress: d.progress, status: d.status } : null;
  }

  async function downloadVod() {
    if (!ch || vodDownload) return;
    try {
      await startDownload(ch.stream_url, `${names!.title}.${extFromUrl(ch.stream_url)}`, ch.id);
      flash('Download started');
    } catch (e) {
      flash(`Download failed: ${e}`);
    }
  }

  async function downloadEpisode(s: SeasonDetail, e: EpisodeDetail) {
    if (!ch || activeDownload((d) => d.url === e.stream_url)) return;
    const sn = s.season_number.padStart(2, '0');
    const en = e.episode_num.padStart(2, '0');
    try {
      await startDownload(e.stream_url, `${names!.title} - S${sn}E${en}.${extFromUrl(e.stream_url)}`, 0);
      flash(`Downloading S${sn}E${en}`);
    } catch (err) {
      flash(`Download failed: ${err}`);
    }
  }

  function back() {
    if (history.length > 1) history.back();
    else goto('/');
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement)) back();
  }

  function episodeLabel(s: SeasonDetail, e: EpisodeDetail): string {
    return `S${s.season_number}:E${e.episode_num}`;
  }

  function episodeTitle(e: EpisodeDetail): string {
    // Many servers repeat the series name ("Show - S01E03 - Title"); keep the useful tail
    const parts = e.title.split(/\s+-\s+/);
    const tail = parts[parts.length - 1]?.trim();
    return tail && !/^S\d+E\d+$/i.test(tail) && parts.length > 1 ? tail : `Episode ${e.episode_num}`;
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="title-page">
  <button class="back" onclick={back} aria-label="Back">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/></svg>
  </button>

  {#if error}
    <div class="state">
      <h1>This title isn't available</h1>
      <p>{error}</p>
      <button class="btn-play" onclick={back}>Go back</button>
    </div>
  {:else if loading || !ch || !names}
    <section class="hero">
      <div class="hero-copy">
        <div class="sk" style="width: 90px; height: 18px"></div>
        <div class="sk" style="width: 70%; height: 72px"></div>
        <div class="sk" style="width: 45%; height: 16px"></div>
        <div class="sk-row"><div class="sk" style="width: 140px; height: 48px"></div><div class="sk" style="width: 48px; height: 48px; border-radius: 50%"></div></div>
      </div>
    </section>
  {:else}
    <!-- Hero -->
    <section class="hero">
      <div class="hero-media" aria-hidden="true">
        {#if poster}<img class="ambient" src={poster} alt="" />{/if}
        {#if artwork}
          {#key artwork}
            <img class="backdrop" class:ready={backdropReady} src={artwork} alt="" onload={(e) => (backdropReady = isWideBackdrop(e.currentTarget as HTMLImageElement))} />
          {/key}
        {/if}
        {#if poster && !backdropReady}
          <img class="poster" src={poster} alt="" out:fade={{ duration: 400 }} />
        {/if}
      </div>
      <div class="hero-shade" aria-hidden="true"></div>

      <div class="hero-copy" in:fade={{ duration: 400 }}>
        <span class="kicker"><b>J</b>{isSeries ? 'SERIES' : 'FILM'}</span>
        {#if tmdb?.logo}
          <h1 class="sr-only">{names.title}</h1>
          <img class="title-logo" src={tmdb.logo} alt="" in:fade={{ duration: 300 }} />
        {:else}
          <h1 class="title" dir="auto">{names.title}</h1>
        {/if}

        <div class="meta">
          {#if rating}<span class="score">{rating.toFixed(1)} rating</span>{/if}
          {#if meta?.year || names.year}<span>{meta?.year ?? names.year}</span>{/if}
          {#if meta?.runtime}<span>{meta.runtime}</span>{/if}
          {#if isSeries && seasons.length}
            <span>{seasons.length} {seasons.length === 1 ? 'Season' : 'Seasons'}</span>
          {/if}
          {#if !isSeries}<span class="badge">{extFromUrl(ch.stream_url).toUpperCase()}</span>{/if}
          {#if meta?.genres.length}<span class="genres">{meta.genres.slice(0, 3).join(' · ')}</span>{/if}
        </div>

        <div class="actions">
          <button class="btn-play" onclick={playMain} disabled={isSeries && metaLoading && !seasons.length}>
            <svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 4.5v15a1 1 0 001.53.85l12-7.5a1 1 0 000-1.7l-12-7.5A1 1 0 007 4.5z"/></svg>
            {#if isSeries && resumeEpisode}
              {resumeEpisode.resumed ? 'Resume' : 'Play'} <small>{episodeLabel(resumeEpisode.season, resumeEpisode.episode)}</small>
            {:else if movieResume}
              Resume <small>{fmtLeft(movieResume.duration - movieResume.pos)}</small>
            {:else}
              Play
            {/if}
          </button>

          {#if trailer}
            <button class="btn-secondary" onclick={playTrailer}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2.5" y="5" width="19" height="14" rx="3"/><path d="M10 9.5v5l4.5-2.5z" fill="currentColor"/></svg>
              Trailer
            </button>
          {/if}

          <button
            class="btn-round"
            class:on={favorite}
            onclick={toggleList}
            aria-label={favorite ? 'Remove from My list' : 'Add to My list'}
            data-tip={favorite ? 'Remove from My list' : 'Add to My list'}
          >
            {#if favorite}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="5 12.5 10 17 19 7"/></svg>
            {:else}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
            {/if}
          </button>

          {#if !isSeries}
            <button
              class="btn-round"
              class:busy={!!vodDownload}
              onclick={downloadVod}
              aria-label="Download"
              data-tip={vodDownload ? `Downloading ${Math.round(vodDownload.progress * 100)}%` : 'Download'}
            >
              {#if vodDownload}
                <svg class="ring" viewBox="0 0 36 36" aria-hidden="true">
                  <circle cx="18" cy="18" r="15" />
                  <circle class="ring-fill" cx="18" cy="18" r="15" style:stroke-dashoffset={94.2 * (1 - vodDownload.progress)} />
                </svg>
                <span class="pct">{Math.round(vodDownload.progress * 100)}</span>
              {:else}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v11"/><polyline points="7 10.5 12 15.5 17 10.5"/><path d="M5 19.5h14"/></svg>
              {/if}
            </button>
          {/if}
        </div>

        {#if plot}
          <p class="plot" dir="auto">{plot}</p>
        {:else if metaLoading}
          <div class="plot-sk"><div class="sk"></div><div class="sk"></div><div class="sk short"></div></div>
        {/if}

        {#if meta?.cast || meta?.director}
          <dl class="credits">
            {#if meta.cast}<div><dt>Cast</dt><dd dir="auto">{meta.cast}</dd></div>{/if}
            {#if meta.director}<div><dt>Director</dt><dd dir="auto">{meta.director}</dd></div>{/if}
          </dl>
        {/if}
      </div>
    </section>

    <div class="body">
      <!-- Episodes -->
      {#if isSeries}
        <section class="episodes">
          <header class="section-head">
            <h2>Episodes</h2>
            {#if seasons.length > 1}
              <label class="season-pick">
                <span class="sr-only">Season</span>
                <select bind:value={season}>
                  {#each seasons as s (s.season_number)}
                    <option value={s.season_number}>Season {s.season_number} ({s.episodes.length})</option>
                  {/each}
                </select>
              </label>
            {:else if seasons.length === 1}
              <span class="season-one">Season {seasons[0].season_number}</span>
            {/if}
          </header>

          {#if metaLoading && !seasons.length}
            {#each Array(4) as _, i (i)}
              <div class="ep-sk"><div class="sk thumb"></div><div class="sk-lines"><div class="sk"></div><div class="sk"></div><div class="sk short"></div></div></div>
            {/each}
          {:else if !seasons.length}
            <p class="empty">No episodes are listed for this series yet.</p>
          {:else}
            {@const s = seasons.find((x) => x.season_number === season)!}
            <ol class="ep-list">
              {#each episodes as e (e.id)}
                {@const dl = activeDownload((d) => d.url === e.stream_url)}
                {@const isResume = resume?.season === s.season_number && resume?.episode === e.episode_num}
                <li class="ep" class:current={isResume}>
                  <button class="ep-main" onclick={() => playEpisode(s, e)}>
                    <span class="ep-num">{e.episode_num}</span>
                    <span class="ep-thumb">
                      {#if e.image || poster}
                        <img src={e.image ?? (backdropReady ? artwork : poster)} alt="" loading="lazy" class:fallback={!e.image} />
                      {/if}
                      <span class="ep-play" aria-hidden="true">
                        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.14v13.72a1 1 0 001.5.86l11-6.86a1 1 0 000-1.72l-11-6.86A1 1 0 008 5.14z"/></svg>
                      </span>
                      {#if isResume}<span class="ep-bar" aria-hidden="true"></span>{/if}
                    </span>
                    <span class="ep-text">
                      <span class="ep-title-row">
                        <span class="ep-title" dir="auto">{episodeTitle(e)}</span>
                        {#if e.duration_secs}<span class="ep-dur">{formatRuntime(e.duration_secs)}</span>{/if}
                      </span>
                      {#if isResume}<span class="ep-tag">Last watched</span>{/if}
                      {#if e.plot}<span class="ep-plot" dir="auto">{e.plot}</span>{/if}
                    </span>
                  </button>
                  <button
                    class="ep-dl"
                    class:busy={!!dl}
                    onclick={() => downloadEpisode(s, e)}
                    aria-label={`Download episode ${e.episode_num}`}
                    title={dl ? `Downloading ${Math.round(dl.progress * 100)}%` : 'Download'}
                  >
                    {#if dl}
                      <svg class="ring" viewBox="0 0 36 36" aria-hidden="true">
                        <circle cx="18" cy="18" r="15" />
                        <circle class="ring-fill" cx="18" cy="18" r="15" style:stroke-dashoffset={94.2 * (1 - dl.progress)} />
                      </svg>
                    {:else}
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v11"/><polyline points="7 10.5 12 15.5 17 10.5"/><path d="M5 19.5h14"/></svg>
                    {/if}
                  </button>
                </li>
              {/each}
            </ol>
          {/if}
        </section>
      {/if}

      <!-- Cast (TMDB) -->
      {#if tmdb?.cast.length}
        <section class="people">
          <header class="section-head"><h2>Cast</h2></header>
          <ul class="people-list">
            {#each tmdb.cast as person (person.name + (person.character ?? ''))}
              <li class="person">
                <span class="face">
                  {#if person.photo}<img src={person.photo} alt="" loading="lazy" />{:else}{person.name.charAt(0)}{/if}
                </span>
                <b>{person.name}</b>
                {#if person.character}<small>{person.character}</small>{/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      <!-- More like this -->
      {#if related.length}
        <section class="more">
          <header class="section-head"><h2>More like this</h2></header>
          <div class="more-grid">
            {#each related as r (r.id)}
              {@const rt = splitTitle(r.name)}
              <a class="more-card" href={`/title?id=${r.id}`} aria-label={rt.title}>
                <span class="more-art">
                  {#if r.logo_url}
                    <img src={r.logo_url} alt="" loading="lazy" />
                  {:else}
                    <span class="more-empty" dir="auto">{rt.title}</span>
                  {/if}
                </span>
                <span class="more-title" dir="auto">{rt.title}</span>
                {#if rt.year}<span class="more-year">{rt.year}</span>{/if}
              </a>
            {/each}
          </div>
        </section>
      {/if}

      <!-- About -->
      <section class="about">
        <header class="section-head"><h2>About <span dir="auto">{names.title}</span></h2></header>
        <dl class="about-list">
          {#if meta?.director}<div><dt>Director</dt><dd dir="auto">{meta.director}</dd></div>{/if}
          {#if meta?.cast}<div><dt>Cast</dt><dd dir="auto">{meta.cast}</dd></div>{/if}
          {#if meta?.genres.length}<div><dt>Genres</dt><dd>{meta.genres.join(', ')}</dd></div>{/if}
          {#if meta?.released}<div><dt>Released</dt><dd>{meta.released}</dd></div>{/if}
          <div><dt>Category</dt><dd dir="auto">{ch.group_name}</dd></div>
          <div><dt>Playlist</dt><dd>{$playlists.find((p) => p.id === ch!.playlist_id)?.name ?? '—'}</dd></div>
        </dl>
      </section>
    </div>
  {/if}

  {#if notice}
    <div class="toast" role="status" transition:fade={{ duration: 180 }}>{notice}</div>
  {/if}
</div>

<style>
  .title-page {
    --gutter: clamp(24px, 3.6vw, 60px);
    position: relative;
    margin: -24px;
    min-height: 100vh;
    padding-bottom: 72px;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }

  .back {
    position: fixed;
    top: 20px;
    left: calc(72px + 20px);
    z-index: 30;
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(0.13 0.004 25 / 0.6);
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.16);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    color: var(--color-text);
    transition: background 200ms var(--ease-out), transform 200ms var(--ease-out);
  }
  .back :global(svg) { width: 22px; height: 22px; }
  .back:hover { background: oklch(0.25 0.005 25 / 0.8); transform: translateX(-2px); }
  .back:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  /* ── Hero ─────────────────────────────────────────────────────────────── */

  .hero {
    position: relative;
    min-height: clamp(560px, 88vh, 940px);
    display: flex;
    align-items: flex-end;
    padding: 120px var(--gutter) 48px;
    isolation: isolate;
    overflow: hidden;
  }

  .hero-media { position: absolute; inset: 0; z-index: -2; }

  .ambient {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    filter: blur(90px) saturate(1.3) brightness(0.4);
    transform: scale(1.4);
  }

  .backdrop {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center 22%;
    opacity: 0;
    transition: opacity 900ms var(--ease-out);
  }
  .backdrop.ready { opacity: 1; }

  .poster {
    position: absolute;
    right: calc(var(--gutter) + 5vw);
    top: 50%;
    height: min(68%, 600px);
    aspect-ratio: 2 / 3;
    object-fit: cover;
    border-radius: 10px;
    transform: translateY(-50%);
    box-shadow: 0 50px 100px -30px oklch(0 0 0 / 0.9), 0 0 0 1px oklch(1 0 0 / 0.1);
  }

  .hero-shade {
    position: absolute;
    inset: 0;
    z-index: -1;
    background:
      linear-gradient(to top, var(--color-base) 0%, oklch(0.165 0.004 25 / 0.7) 22%, transparent 55%),
      linear-gradient(80deg, oklch(0.165 0.004 25 / 0.95) 0%, oklch(0.165 0.004 25 / 0.6) 40%, transparent 72%),
      linear-gradient(to bottom, oklch(0.1 0.004 25 / 0.6) 0%, transparent 20%);
  }

  .hero-copy {
    width: min(680px, 52vw);
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 16px;
  }

  .kicker {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    font-size: 0.75rem;
    font-weight: 700;
    letter-spacing: 0.32em;
    color: var(--color-text);
  }
  .kicker b {
    font-size: 1.5rem;
    font-weight: 900;
    letter-spacing: -0.04em;
    color: var(--color-accent);
  }

  .title {
    font-size: clamp(2.5rem, 5vw, 4.75rem);
    font-weight: 900;
    line-height: 1.04;
    letter-spacing: -0.035em;
    text-wrap: balance;
    text-shadow: 0 2px 30px oklch(0 0 0 / 0.5);
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 14px;
    font-size: 1rem;
    font-weight: 500;
    color: oklch(0.88 0.004 25);
  }
  .meta .score { color: var(--color-accent-green); font-weight: 700; }
  .meta .badge {
    padding: 0 6px;
    border: 1px solid oklch(1 0 0 / 0.45);
    border-radius: 3px;
    font-size: 0.6875rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    line-height: 1.6;
  }
  .meta .genres { color: var(--color-text-muted); }

  .actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
    margin: 6px 0 4px;
  }

  .btn-play,
  .btn-secondary {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    height: 50px;
    padding: 0 28px 0 22px;
    border-radius: 6px;
    font-size: 1.0625rem;
    font-weight: 700;
    transition: background 200ms var(--ease-out), transform 120ms var(--ease-out);
  }
  .btn-play :global(svg),
  .btn-secondary :global(svg) { width: 26px; height: 26px; }
  .btn-play small { font-size: 0.875rem; font-weight: 600; opacity: 0.7; }
  .btn-play { background: var(--color-text); color: oklch(0.14 0.004 25); }
  .btn-play:hover:not(:disabled) { background: oklch(0.85 0.004 25); }
  .btn-secondary { background: oklch(0.55 0.004 25 / 0.55); color: var(--color-text); }
  .btn-secondary:hover { background: oklch(0.55 0.004 25 / 0.38); }
  .btn-play:active, .btn-secondary:active, .btn-round:active { transform: scale(0.96); }
  .btn-play:focus-visible, .btn-secondary:focus-visible, .btn-round:focus-visible {
    outline: 2px solid var(--color-text);
    outline-offset: 3px;
  }

  .btn-round {
    position: relative;
    width: 50px;
    height: 50px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: oklch(0.16 0.004 25 / 0.6);
    box-shadow: inset 0 0 0 2px oklch(1 0 0 / 0.5);
    color: var(--color-text);
    transition: box-shadow 200ms var(--ease-out), background 200ms var(--ease-out), transform 120ms var(--ease-out);
  }
  .btn-round > :global(svg) { width: 24px; height: 24px; }
  .btn-round:hover { box-shadow: inset 0 0 0 2px var(--color-text); background: oklch(0.25 0.005 25 / 0.7); }
  .btn-round.on { box-shadow: inset 0 0 0 2px var(--color-text); }

  .btn-round[data-tip]::after {
    content: attr(data-tip);
    position: absolute;
    bottom: calc(100% + 10px);
    left: 50%;
    transform: translate(-50%, 4px);
    padding: 6px 10px;
    border-radius: 4px;
    background: var(--color-text);
    color: oklch(0.14 0.004 25);
    font-size: 0.8125rem;
    font-weight: 700;
    white-space: nowrap;
    opacity: 0;
    pointer-events: none;
    transition: opacity 150ms var(--ease-out), transform 150ms var(--ease-out);
  }
  .btn-round:hover::after,
  .btn-round:focus-visible::after { opacity: 1; transform: translate(-50%, 0); }

  .ring { position: absolute; inset: 3px; width: calc(100% - 6px) !important; height: calc(100% - 6px) !important; transform: rotate(-90deg); }
  .ring circle { fill: none; stroke: oklch(1 0 0 / 0.15); stroke-width: 3; }
  .ring .ring-fill { stroke: var(--color-accent); stroke-dasharray: 94.2; transition: stroke-dashoffset 400ms var(--ease-out); }
  .pct { font-size: 0.75rem; font-weight: 800; font-variant-numeric: tabular-nums; }

  .plot {
    font-size: 1.125rem;
    line-height: 1.55;
    color: oklch(0.92 0.004 25);
    max-width: 62ch;
    text-shadow: 0 1px 14px oklch(0 0 0 / 0.5);
    display: -webkit-box;
    -webkit-line-clamp: 5;
    line-clamp: 5;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .plot:dir(rtl) { font-size: 1.1875rem; line-height: 1.85; }
  .title:dir(rtl) { line-height: 1.25; letter-spacing: 0; }
  .ep-plot:dir(rtl) { font-size: 0.9375rem; line-height: 1.8; }
  .credits dd:dir(rtl), .about-list dd:dir(rtl) { line-height: 1.8; }

  .plot-sk { width: 100%; display: flex; flex-direction: column; gap: 10px; }
  .plot-sk .sk { height: 15px; }
  .plot-sk .short, .sk-lines .short { width: 60%; }

  .credits {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 0.875rem;
    max-width: 62ch;
  }
  .credits div { display: flex; gap: 8px; }
  .credits dt { color: var(--color-text-muted); flex-shrink: 0; }
  .credits dd {
    color: oklch(0.88 0.004 25);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Body ─────────────────────────────────────────────────────────────── */

  .body {
    display: flex;
    flex-direction: column;
    gap: 56px;
    padding: 8px var(--gutter) 0;
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }
  .section-head h2 {
    font-size: 1.5rem;
    font-weight: 800;
    letter-spacing: -0.02em;
  }

  /* Episodes */
  .episodes { max-width: 1180px; }

  .season-pick select {
    appearance: none;
    height: 42px;
    padding: 0 40px 0 16px;
    border-radius: 6px;
    border: 1px solid oklch(1 0 0 / 0.25);
    background:
      url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%23eee' stroke-width='2.5'%3E%3Cpolyline points='6 9 12 15 18 9'/%3E%3C/svg%3E") no-repeat right 14px center / 14px,
      oklch(0.22 0.005 25);
    color: var(--color-text);
    font-size: 0.9375rem;
    font-weight: 700;
    cursor: pointer;
  }
  .season-pick select:hover { border-color: oklch(1 0 0 / 0.5); }
  .season-pick select:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }
  .season-one { font-size: 0.9375rem; font-weight: 700; color: var(--color-text-muted); }

  .ep-list { list-style: none; border-top: 1px solid oklch(1 0 0 / 0.08); }

  .ep {
    display: flex;
    align-items: center;
    border-bottom: 1px solid oklch(1 0 0 / 0.08);
    border-radius: 6px;
    transition: background 200ms var(--ease-out);
  }
  .ep:hover, .ep.current { background: oklch(1 0 0 / 0.045); }

  .ep-main {
    flex: 1;
    min-width: 0;
    display: grid;
    grid-template-columns: 44px clamp(150px, 15vw, 220px) 1fr;
    align-items: center;
    gap: 20px;
    padding: 18px 8px 18px 12px;
    background: none;
    color: var(--color-text);
    text-align: start;
    border-radius: 6px;
  }
  .ep-main:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }

  .ep-num {
    font-size: 1.5rem;
    font-weight: 600;
    color: var(--color-text-muted);
    text-align: center;
    font-variant-numeric: tabular-nums;
  }

  .ep-thumb {
    position: relative;
    aspect-ratio: 16 / 9;
    border-radius: 5px;
    overflow: hidden;
    background: var(--color-card);
  }
  .ep-thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .ep-thumb img.fallback { filter: brightness(0.6) saturate(0.8); }

  .ep-play {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: oklch(0 0 0 / 0.35);
    opacity: 0;
    transition: opacity 200ms var(--ease-out);
  }
  .ep-play :global(svg) {
    width: 40px;
    height: 40px;
    padding: 10px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 2px var(--color-text);
    background: oklch(0 0 0 / 0.5);
    color: var(--color-text);
  }
  .ep-main:hover .ep-play, .ep-main:focus-visible .ep-play { opacity: 1; }

  .ep-bar {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
    background: var(--color-accent);
  }

  .ep-text { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .ep-title-row { display: flex; align-items: baseline; justify-content: space-between; gap: 16px; }
  .ep-title { font-size: 1rem; font-weight: 700; }
  .ep-dur { flex-shrink: 0; font-size: 0.875rem; color: var(--color-text-muted); font-variant-numeric: tabular-nums; }
  .ep-tag {
    align-self: flex-start;
    font-size: 0.6875rem;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-accent-soft);
  }
  .ep-plot {
    font-size: 0.875rem;
    line-height: 1.5;
    color: var(--color-text-muted);
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .ep-dl {
    position: relative;
    flex-shrink: 0;
    width: 42px;
    height: 42px;
    margin: 0 14px 0 4px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: none;
    color: var(--color-text-muted);
    box-shadow: inset 0 0 0 1.5px oklch(1 0 0 / 0.25);
    transition: color 200ms var(--ease-out), box-shadow 200ms var(--ease-out);
  }
  .ep-dl > :global(svg) { width: 20px; height: 20px; }
  .ep-dl:hover { color: var(--color-text); box-shadow: inset 0 0 0 1.5px var(--color-text); }
  .ep-dl:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }

  .ep-sk { display: grid; grid-template-columns: clamp(150px, 15vw, 220px) 1fr; gap: 20px; padding: 18px 12px 18px 76px; }
  .ep-sk .thumb { aspect-ratio: 16 / 9; }
  .sk-lines { display: flex; flex-direction: column; gap: 10px; justify-content: center; }
  .sk-lines .sk { height: 14px; }

  .empty { color: var(--color-text-muted); padding: 24px 0; }

  .title-logo {
    max-width: min(520px, 90%);
    max-height: 180px;
    object-fit: contain;
    object-position: left bottom;
    filter: drop-shadow(0 6px 24px oklch(0 0 0 / 0.55));
  }

  /* Cast */
  .people-list {
    list-style: none;
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 112px;
    gap: 16px;
    overflow-x: auto;
    padding-bottom: 10px;
    scrollbar-width: thin;
  }
  .person { display: flex; flex-direction: column; align-items: center; gap: 4px; text-align: center; min-width: 0; }
  .face {
    width: 96px;
    height: 96px;
    margin-bottom: 6px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    overflow: hidden;
    background: var(--color-card);
    font-size: 1.75rem;
    font-weight: 800;
    color: var(--color-text-muted);
    box-shadow: 0 0 0 1px oklch(1 0 0 / 0.08);
  }
  .face img { width: 100%; height: 100%; object-fit: cover; }
  .person b { font-size: 0.8125rem; font-weight: 700; max-width: 100%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .person small { font-size: 0.75rem; color: var(--color-text-muted); max-width: 100%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  /* More like this */
  .more-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 28px 12px;
  }

  .more-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    color: var(--color-text);
    text-decoration: none;
    border-radius: 6px;
  }
  .more-card:focus-visible { outline: none; }
  .more-card:focus-visible .more-art { outline: 2px solid var(--color-text); outline-offset: 2px; }

  .more-art {
    display: block;
    aspect-ratio: 2 / 3;
    border-radius: 6px;
    overflow: hidden;
    background: var(--color-card);
    transition: transform 240ms var(--ease-out), box-shadow 240ms var(--ease-out);
  }
  .more-art img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .more-card:hover .more-art {
    transform: translateY(-4px) scale(1.03);
    box-shadow: 0 18px 40px -12px oklch(0 0 0 / 0.8);
  }
  .more-empty {
    display: flex;
    align-items: flex-end;
    width: 100%;
    height: 100%;
    padding: 12px;
    font-weight: 800;
    background: radial-gradient(120% 80% at 100% 0%, oklch(0.42 0.16 27 / 0.55), transparent 60%), var(--color-card);
  }
  .more-title {
    font-size: 0.8125rem;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .more-year { font-size: 0.75rem; color: var(--color-text-muted); margin-top: -6px; }

  /* About */
  .about-list {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 14px 40px;
    font-size: 0.9375rem;
  }
  .about-list div { display: flex; flex-direction: column; gap: 3px; }
  .about-list dt { color: var(--color-text-muted); font-size: 0.8125rem; }
  .about-list dd { line-height: 1.5; }

  /* States */
  .state {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    gap: 14px;
    padding: 0 var(--gutter);
  }
  .state h1 { font-size: 2.5rem; font-weight: 900; letter-spacing: -0.03em; }
  .state p { color: var(--color-text-muted); max-width: 60ch; }

  .sk {
    border-radius: 4px;
    background: linear-gradient(90deg, oklch(1 0 0 / 0.05) 0%, oklch(1 0 0 / 0.1) 50%, oklch(1 0 0 / 0.05) 100%);
    background-size: 200% 100%;
    animation: shimmer 1.6s linear infinite;
  }
  .sk-row { display: flex; gap: 12px; margin-top: 8px; }
  @keyframes shimmer {
    from { background-position: 100% 0; }
    to { background-position: -100% 0; }
  }

  .toast {
    position: fixed;
    left: 50%;
    bottom: 32px;
    z-index: 50;
    transform: translateX(-50%);
    padding: 12px 20px;
    border-radius: 6px;
    background: var(--color-text);
    color: oklch(0.14 0.004 25);
    font-size: 0.9375rem;
    font-weight: 700;
    box-shadow: 0 20px 40px -12px oklch(0 0 0 / 0.7);
  }

  @media (max-width: 1100px) {
    .hero-copy { width: min(680px, 80vw); }
    .poster { display: none; }
  }

  @media (prefers-reduced-motion: reduce) {
    .sk { animation: none; }
    .backdrop, .more-art, .ep-play { transition: none; }
  }
</style>
