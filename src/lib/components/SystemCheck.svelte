<script lang="ts" module>
  import type { SystemStatus } from '$lib/tauri';

  type Need = 'required' | 'recommended' | 'optional';
  const ABOUT: Record<string, { name: string; does: string; need: Need }> = {
    mpv: { name: 'mpv', does: 'Plays live TV, movies and series', need: 'required' },
    ffmpeg: { name: 'FFmpeg', does: 'Joins downloaded video and audio, makes library thumbnails', need: 'recommended' },
    ffprobe: { name: 'FFprobe', does: 'Reads the length and size of your videos', need: 'recommended' },
    'yt-dlp': { name: 'yt-dlp', does: 'Saves videos from YouTube, TikTok, Instagram and more', need: 'optional' },
    xwayland: { name: 'XWayland', does: 'Shows video inside the JoTV window (otherwise it opens in its own)', need: 'recommended' },
    x11: { name: 'X11 display', does: 'Shows video inside the JoTV window (otherwise it opens in its own)', need: 'recommended' },
  };

  type Family = 'arch' | 'debian' | 'fedora' | 'suse';
  const FAMILIES: { id: Family; label: string; install: string; ids: string[]; pkg: Record<string, string> }[] = [
    {
      id: 'arch', label: 'Arch', install: 'sudo pacman -S --needed',
      ids: ['arch', 'cachyos', 'manjaro', 'endeavouros', 'garuda'],
      pkg: { mpv: 'mpv', ffmpeg: 'ffmpeg', ffprobe: 'ffmpeg', 'yt-dlp': 'yt-dlp', xwayland: 'xorg-xwayland' },
    },
    {
      id: 'debian', label: 'Ubuntu · Debian', install: 'sudo apt install',
      ids: ['debian', 'ubuntu', 'linuxmint', 'pop', 'elementary', 'zorin'],
      pkg: { mpv: 'mpv', ffmpeg: 'ffmpeg', ffprobe: 'ffmpeg', 'yt-dlp': 'yt-dlp', xwayland: 'xwayland' },
    },
    {
      id: 'fedora', label: 'Fedora', install: 'sudo dnf install',
      ids: ['fedora', 'rhel', 'centos', 'nobara'],
      pkg: { mpv: 'mpv', ffmpeg: 'ffmpeg-free', ffprobe: 'ffmpeg-free', 'yt-dlp': 'yt-dlp', xwayland: 'xorg-x11-server-Xwayland' },
    },
    {
      id: 'suse', label: 'openSUSE', install: 'sudo zypper install',
      ids: ['opensuse', 'suse', 'opensuse-tumbleweed', 'opensuse-leap'],
      pkg: { mpv: 'mpv', ffmpeg: 'ffmpeg', ffprobe: 'ffmpeg', 'yt-dlp': 'yt-dlp', xwayland: 'xwayland' },
    },
  ];

  function familyOf(s: SystemStatus): Family | null {
    for (const id of s.distro) {
      const f = FAMILIES.find((x) => x.ids.includes(id));
      if (f) return f.id;
    }
    return null;
  }

  // One check per app run: the first screen and Settings share it
  let cached: SystemStatus | null = null;
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import { systemCheck } from '$lib/tauri';

  /** First-screen version: one line when all is well, the fix when it isn't */
  let { compact = false }: { compact?: boolean } = $props();

  let status = $state<SystemStatus | null>(cached);
  let checking = $state(false);
  let family = $state<Family>('arch');
  let detected = $state<Family | null>(null);
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;

  async function run() {
    checking = true;
    try {
      status = cached = await systemCheck();
      detected = familyOf(status);
      if (detected) family = detected;
    } catch {
      status = null;
    } finally {
      checking = false;
    }
  }

  onMount(() => {
    if (cached) {
      detected = familyOf(cached);
      if (detected) family = detected;
    } else {
      run();
    }
    return () => clearTimeout(copyTimer);
  });

  let tools = $derived((status?.tools ?? []).filter((t) => ABOUT[t.id]));
  let missing = $derived(tools.filter((t) => !t.found));
  let missingRequired = $derived(missing.filter((t) => ABOUT[t.id].need === 'required'));
  let linux = $derived(status?.os === 'linux');
  let fam = $derived(FAMILIES.find((f) => f.id === family)!);
  // Installable from the package manager (a missing X11 display isn't)
  let packages = $derived([...new Set(missing.map((t) => fam.pkg[t.id]).filter(Boolean))]);
  let command = $derived(packages.length ? `${fam.install} ${packages.join(' ')}` : '');

  let headline = $derived(
    !missing.length
      ? 'Everything JoTV needs is installed'
      : missingRequired.length
        ? `${ABOUT[missingRequired[0].id].name} is missing: JoTV can't play video without it`
        : `${missing.length} optional ${missing.length === 1 ? 'program is' : 'programs are'} missing`,
  );

  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
    } catch {
      // Older webviews: copy through a hidden field
      const field = document.createElement('textarea');
      field.value = command;
      field.style.position = 'fixed';
      field.style.opacity = '0';
      document.body.appendChild(field);
      field.select();
      document.execCommand('copy');
      field.remove();
    }
    copied = true;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => (copied = false), 1800);
  }
</script>

{#if status && !(compact && !linux)}
  <section class="syscheck" class:compact class:bad={missingRequired.length > 0} class:warn={missing.length > 0 && !missingRequired.length} aria-label="Programs JoTV uses">
    <header class="head">
      <span class="badge" aria-hidden="true">
        {#if !missing.length}
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="5 12.5 10 17 19 7"/></svg>
        {:else}
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><line x1="12" y1="7" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
        {/if}
      </span>
      <div class="head-text">
        <strong>{headline}</strong>
        {#if !compact || missing.length}
          <span>
            {#if status.os === 'windows'}
              mpv, FFmpeg and yt-dlp come with JoTV, nothing to install.
            {:else if status.distro_name}
              {status.distro_name}
            {:else}
              {status.os}
            {/if}
          </span>
        {/if}
      </div>
      <button class="again" onclick={run} disabled={checking}>
        {#if checking}<span class="spin" aria-hidden="true"></span>Checking…{:else}Check again{/if}
      </button>
    </header>

    {#if !compact || missing.length}
      <ul class="tools">
        {#each tools as t (t.id)}
          {@const a = ABOUT[t.id]}
          {#if !compact || !t.found}
            <li class:ok={t.found}>
              <span class="dot {t.found ? 'ok' : a.need}" aria-hidden="true"></span>
              <div class="tool-text">
                <b>{a.name}</b>
                <span>{a.does}</span>
              </div>
              <span class="state">
                {#if t.found}
                  {#if t.bundled}Included{:else if t.version}v{t.version}{:else}Ready{/if}
                {:else if a.need === 'required'}
                  Needed
                {:else if a.need === 'recommended'}
                  Recommended
                {:else}
                  Optional
                {/if}
              </span>
              {#if !compact && t.found && t.path && t.id !== 'x11' && t.id !== 'xwayland'}
                <code class="path" title={t.path}>{t.path}</code>
              {/if}
            </li>
          {/if}
        {/each}
      </ul>
    {/if}

    {#if linux && command}
      <div class="fix">
        <div class="fix-head">
          <span>Install {packages.length === 1 ? 'it' : 'them'} with one command</span>
          <div class="fams" role="tablist" aria-label="Your Linux">
            {#each FAMILIES as f (f.id)}
              <button
                role="tab"
                aria-selected={family === f.id}
                class:on={family === f.id}
                onclick={() => (family = f.id)}
                title={detected === f.id ? 'Your system' : undefined}
              >
                {f.label}{#if detected === f.id}<i aria-label="your system"></i>{/if}
              </button>
            {/each}
          </div>
        </div>
        <div class="cmd">
          <code dir="ltr"><span class="prompt" aria-hidden="true">$</span>{command}</code>
          <button class="copy" class:done={copied} onclick={copy} aria-label="Copy command">
            {#if copied}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="5 12.5 10 17 19 7"/></svg>
              Copied
            {:else}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><rect x="8.5" y="8.5" width="12" height="12" rx="2.5"/><path d="M15.5 8.5V6a2.5 2.5 0 00-2.5-2.5H6A2.5 2.5 0 003.5 6v7A2.5 2.5 0 006 15.5h2.5"/></svg>
              Copy
            {/if}
          </button>
        </div>
        <p class="fix-hint">
          Paste it in a terminal, enter your password, then press <b>Check again</b>.
          {#if family === 'debian' && packages.includes('yt-dlp')}
            Ubuntu's yt-dlp is often too old for YouTube; <code>pipx install yt-dlp</code> gets the current one.
          {:else if family === 'fedora' && packages.includes('ffmpeg-free')}
            For more video formats, use <code>ffmpeg</code> from RPM Fusion instead of <code>ffmpeg-free</code>.
          {/if}
        </p>
      </div>
    {/if}
  </section>
{/if}

<style>
  .syscheck {
    --tone: var(--color-accent-green);
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 18px;
    border-radius: 14px;
    background: var(--color-surface);
    box-shadow: inset 0 0 0 1px var(--color-border);
  }
  .syscheck.warn { --tone: var(--color-accent-yellow); }
  .syscheck.bad { --tone: var(--color-accent-soft); }
  .syscheck.compact { padding: 14px 16px; gap: 12px; }

  .head { display: flex; align-items: center; gap: 12px; }
  .badge {
    flex-shrink: 0;
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--tone);
    background: color-mix(in oklch, var(--tone) 16%, transparent);
  }
  .badge :global(svg) { width: 18px; height: 18px; }
  .compact .badge { width: 28px; height: 28px; }
  .compact .badge :global(svg) { width: 15px; height: 15px; }
  .head-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .head-text strong { font-size: 0.9375rem; font-weight: 700; color: var(--color-text); }
  .head-text span { font-size: 0.8125rem; color: var(--color-text-muted); }

  .again {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 13px;
    border-radius: 16px;
    background: oklch(var(--n1000) / 0.08);
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 700;
    transition: background 150ms var(--ease-out);
  }
  .again:hover:not(:disabled) { background: oklch(var(--n1000) / 0.14); }
  .again:disabled { color: var(--color-text-muted); }
  .spin {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid oklch(var(--n1000) / 0.2);
    border-top-color: currentColor;
    animation: spin 800ms linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .tools { list-style: none; display: flex; flex-direction: column; }
  .tools li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    column-gap: 12px;
    padding: 10px 2px;
    border-top: 1px solid oklch(var(--n1000) / 0.06);
  }
  .tools li:first-child { border-top: none; }
  .dot { width: 9px; height: 9px; border-radius: 50%; }
  .dot.ok { background: var(--color-accent-green); }
  .dot.required { background: var(--color-accent); box-shadow: 0 0 0 3px oklch(0.58 0.225 27 / 0.22); }
  .dot.recommended { background: var(--color-accent-yellow); }
  .dot.optional { background: oklch(var(--n1000) / 0.3); }
  .tool-text { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .tool-text b { font-size: 0.875rem; font-weight: 700; color: var(--color-text); }
  .tool-text span { font-size: 0.8125rem; color: var(--color-text-muted); }
  .state {
    font-size: 0.75rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: var(--color-text-muted);
  }
  li:not(.ok) .state { color: var(--color-text); }
  .path {
    grid-column: 2 / -1;
    margin-top: 3px;
    font-size: 0.6875rem;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .fix {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 14px;
    border-top: 1px solid oklch(var(--n1000) / 0.08);
  }
  .fix-head { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px 14px; }
  .fix-head > span { font-size: 0.8125rem; font-weight: 700; color: var(--color-text); }
  .fams { display: flex; flex-wrap: wrap; gap: 4px; padding: 3px; border-radius: 10px; background: oklch(var(--n1000) / 0.05); }
  .fams button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border-radius: 7px;
    background: none;
    color: var(--color-text-muted);
    font-size: 0.75rem;
    font-weight: 700;
    transition: background 120ms var(--ease-out), color 120ms var(--ease-out);
  }
  .fams button:hover { color: var(--color-text); }
  .fams button.on { background: var(--color-text); color: var(--color-base); }
  .fams i { width: 6px; height: 6px; border-radius: 50%; background: var(--color-accent-green); }

  /* The command always reads as a terminal, in either theme */
  .cmd {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 6px 6px 14px;
    border-radius: 10px;
    background: oklch(0.13 0.004 25);
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.08);
  }
  .cmd code {
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    white-space: nowrap;
    font-family: ui-monospace, 'JetBrains Mono', 'Cascadia Code', Menlo, monospace;
    font-size: 0.8125rem;
    color: oklch(0.94 0.003 25);
    scrollbar-width: none;
  }
  .prompt { margin-right: 10px; color: var(--color-accent-green); user-select: none; }
  .copy {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 12px;
    border-radius: 7px;
    background: oklch(0.96 0.003 25);
    color: oklch(0.14 0.004 25);
    font-size: 0.8125rem;
    font-weight: 800;
    transition: background 150ms var(--ease-out);
  }
  .copy :global(svg) { width: 15px; height: 15px; }
  .copy:hover { background: oklch(0.85 0.004 25); }
  .copy.done { background: var(--color-accent-green); color: oklch(0.14 0.004 25); }
  .fix-hint { font-size: 0.8125rem; line-height: 1.5; color: var(--color-text-muted); }
  .fix-hint b { color: var(--color-text); }
  .fix-hint code {
    padding: 1px 6px;
    border-radius: 4px;
    background: oklch(var(--n1000) / 0.08);
    font-family: ui-monospace, 'JetBrains Mono', Menlo, monospace;
    font-size: 0.75rem;
    color: var(--color-text);
  }

  @media (prefers-reduced-motion: reduce) {
    .spin { animation: none; }
  }
</style>
