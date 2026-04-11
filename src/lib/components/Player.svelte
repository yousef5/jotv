<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import Hls from 'hls.js';

  let { streamUrl, onTimeUpdate = (_t: number) => {} }: {
    streamUrl: string;
    onTimeUpdate?: (time: number) => void;
  } = $props();

  let videoEl: HTMLVideoElement | undefined = $state();
  let hls: Hls | null = null;

  function isHlsStream(url: string): boolean {
    return url.includes('.m3u8') || url.includes('/live/') || url.includes('m3u8');
  }

  function setupPlayer() {
    if (!videoEl || !streamUrl) return;
    destroyHls();

    if (isHlsStream(streamUrl)) {
      if (Hls.isSupported()) {
        hls = new Hls({
          enableWorker: true,
          lowLatencyMode: true,
          backBufferLength: 90,
        });
        hls.loadSource(streamUrl);
        hls.attachMedia(videoEl);
        hls.on(Hls.Events.MANIFEST_PARSED, () => {
          videoEl?.play().catch(() => {});
        });
        hls.on(Hls.Events.ERROR, (_event, data) => {
          if (data.fatal) {
            switch (data.type) {
              case Hls.ErrorTypes.NETWORK_ERROR:
                hls?.startLoad();
                break;
              case Hls.ErrorTypes.MEDIA_ERROR:
                hls?.recoverMediaError();
                break;
              default:
                destroyHls();
                break;
            }
          }
        });
      } else if (videoEl.canPlayType('application/vnd.apple.mpegurl')) {
        // Safari native HLS
        videoEl.src = streamUrl;
        videoEl.play().catch(() => {});
      }
    } else {
      videoEl.src = streamUrl;
      videoEl.play().catch(() => {});
    }
  }

  function destroyHls() {
    if (hls) {
      hls.destroy();
      hls = null;
    }
  }

  function handleTimeUpdate() {
    if (videoEl) {
      onTimeUpdate(videoEl.currentTime);
    }
  }

  $effect(() => {
    if (streamUrl && videoEl) {
      setupPlayer();
    }
  });

  onDestroy(() => {
    destroyHls();
  });
</script>

<video
  bind:this={videoEl}
  ontimeupdate={handleTimeUpdate}
  autoplay
  playsinline
  class="player-video"
></video>

<style>
  .player-video {
    width: 100%;
    height: 100%;
    object-fit: contain;
    background: #000;
  }
</style>
