<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import Player from '$lib/components/Player.svelte';
  import PlayerControls from '$lib/components/PlayerControls.svelte';
  import { currentChannel, isPlaying, viewStartTime } from '$lib/stores/player';
  import { recordViewing, getCurrentProgram } from '$lib/tauri';
  import type { EpgEntry } from '$lib/tauri';
  import { get } from 'svelte/store';

  let channel = $state(get(currentChannel));
  let program: EpgEntry | null = $state(null);
  let videoEl: HTMLVideoElement | undefined = $state();
  let epgInterval: ReturnType<typeof setInterval> | null = null;

  onMount(() => {
    if (!channel) {
      goto('/');
      return;
    }

    // Load EPG if available
    if (channel.epg_id) {
      loadEpg(channel.epg_id);
      epgInterval = setInterval(() => {
        if (channel?.epg_id) loadEpg(channel.epg_id);
      }, 60000);
    }
  });

  async function loadEpg(epgId: string) {
    try {
      program = await getCurrentProgram(epgId);
    } catch { /* ignore */ }
  }

  onDestroy(async () => {
    if (epgInterval) clearInterval(epgInterval);

    // Record viewing duration
    const startTime = get(viewStartTime);
    if (channel && startTime) {
      const duration = Math.floor((Date.now() - startTime) / 1000);
      if (duration > 5) {
        try {
          await recordViewing(channel.id, duration);
        } catch { /* ignore */ }
      }
    }
    isPlaying.set(false);
    viewStartTime.set(null);
  });

  // Get reference to video element from the Player component's DOM
  function handleTimeUpdate(_time: number) {
    // Could be used for progress tracking
  }

  $effect(() => {
    // Find the video element after mount
    const el = document.querySelector('.player-video') as HTMLVideoElement | null;
    if (el) videoEl = el;
  });
</script>

{#if channel}
  <div class="cinema">
    <Player streamUrl={channel.stream_url} onTimeUpdate={handleTimeUpdate} />
    <PlayerControls {channel} currentProgram={program} {videoEl} />
  </div>
{/if}

<style>
  .cinema {
    position: relative;
    width: 100vw;
    height: 100vh;
    background: #000;
    overflow: hidden;
  }
</style>
