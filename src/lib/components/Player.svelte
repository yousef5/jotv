<script lang="ts">
  import { onDestroy } from 'svelte';
  import Hls from 'hls.js';
  import mpegts from 'mpegts.js';

  let { streamUrl, onTimeUpdate = (_t: number) => {} }: {
    streamUrl: string;
    onTimeUpdate?: (time: number) => void;
  } = $props();

  let videoEl: HTMLVideoElement | undefined = $state();
  let hls: Hls | null = null;
  let mpegtsPlayer: mpegts.Player | null = null;

  type StreamType = 'hls' | 'ts' | 'direct';

  const VOD_EXTENSIONS = ['.mkv', '.avi', '.flv', '.wmv', '.mov', '.webm', '.mp4'];

  function detectStreamType(url: string): StreamType {
    const lower = url.toLowerCase();
    if (lower.endsWith('.m3u8') || lower.includes('.m3u8')) return 'hls';
    if (lower.endsWith('.ts')) return 'ts';
    return 'direct';
  }

  function toHlsUrl(url: string): string {
    const lower = url.toLowerCase();
    for (const ext of VOD_EXTENSIONS) {
      if (lower.endsWith(ext)) return url.slice(0, url.length - ext.length) + '.m3u8';
    }
    return url;
  }

  function setupPlayer() {
    if (!videoEl || !streamUrl) return;
    destroyPlayer();

    const type = detectStreamType(streamUrl);

    if (type === 'ts' && mpegts.isSupported()) {
      mpegtsPlayer = mpegts.createPlayer({
        type: 'mpegts',
        isLive: true,
        url: streamUrl,
      }, {
        enableWorker: true,
        enableStashBuffer: true,
        stashInitialSize: 1024 * 1024,
        autoCleanupSourceBuffer: true,
        autoCleanupMaxBackwardDuration: 60,
        autoCleanupMinBackwardDuration: 30,
        fixAudioTimestampGap: true,
        lazyLoad: true,
        lazyLoadMaxDuration: 120,
        lazyLoadRecoverDuration: 30,
      });
      mpegtsPlayer.attachMediaElement(videoEl);
      mpegtsPlayer.load();
      videoEl.play().catch(() => {});
    } else if (type === 'hls' && Hls.isSupported()) {
      hls = new Hls({
        enableWorker: true,
        lowLatencyMode: false,
        maxBufferLength: 60,
        maxMaxBufferLength: 120,
        maxBufferSize: 120 * 1000 * 1000,
        maxBufferHole: 0.3,
        backBufferLength: 60,
        liveSyncDurationCount: 3,
        liveMaxLatencyDurationCount: 10,
        liveDurationInfinity: true,
        startLevel: -1,
        abrEwmaDefaultEstimate: 8_000_000,
        abrBandWidthUpFactor: 0.6,
        abrBandWidthFactor: 0.8,
        abrMaxWithRealBitrate: true,
        startFragPrefetch: true,
        progressive: true,
        manifestLoadingMaxRetry: 10,
        manifestLoadingRetryDelay: 300,
        levelLoadingMaxRetry: 10,
        levelLoadingRetryDelay: 300,
        fragLoadingMaxRetry: 10,
        fragLoadingRetryDelay: 300,
        capLevelOnFPSDrop: true,
      });
      hls.loadSource(streamUrl);
      hls.attachMedia(videoEl);
      hls.on(Hls.Events.MANIFEST_PARSED, () => {
        videoEl?.play().catch(() => {});
      });
      hls.on(Hls.Events.ERROR, (_event, data) => {
        if (!data.fatal) return;
        if (data.type === Hls.ErrorTypes.NETWORK_ERROR) {
          hls?.startLoad();
        } else if (data.type === Hls.ErrorTypes.MEDIA_ERROR) {
          hls?.recoverMediaError();
        } else {
          destroyPlayer();
        }
      });
    } else if (type === 'hls' && videoEl.canPlayType('application/vnd.apple.mpegurl')) {
      videoEl.src = streamUrl;
      videoEl.play().catch(() => {});
    } else if (type === 'direct') {
      const hlsUrl = toHlsUrl(streamUrl);
      if (hlsUrl !== streamUrl && Hls.isSupported()) {
        hls = new Hls({
          enableWorker: true,
          maxBufferLength: 60,
          maxMaxBufferLength: 120,
          maxBufferSize: 120 * 1000 * 1000,
          startLevel: -1,
          abrEwmaDefaultEstimate: 8_000_000,
          progressive: true,
          fragLoadingMaxRetry: 10,
          fragLoadingRetryDelay: 300,
        });
        hls.loadSource(hlsUrl);
        hls.attachMedia(videoEl);
        hls.on(Hls.Events.MANIFEST_PARSED, () => { videoEl?.play().catch(() => {}); });
        hls.on(Hls.Events.ERROR, (_event, data) => {
          if (!data.fatal) return;
          if (data.type === Hls.ErrorTypes.NETWORK_ERROR) hls?.startLoad();
          else if (data.type === Hls.ErrorTypes.MEDIA_ERROR) hls?.recoverMediaError();
          else destroyPlayer();
        });
      } else {
        videoEl.src = streamUrl;
        videoEl.play().catch(() => {});
      }
    } else {
      videoEl.src = streamUrl;
      videoEl.play().catch(() => {});
    }
  }

  function destroyPlayer() {
    if (hls) { hls.destroy(); hls = null; }
    if (mpegtsPlayer) {
      try {
        mpegtsPlayer.pause();
        mpegtsPlayer.unload();
        mpegtsPlayer.detachMediaElement();
        mpegtsPlayer.destroy();
      } catch {}
      mpegtsPlayer = null;
    }
    if (videoEl) videoEl.removeAttribute('src');
  }

  function handleTimeUpdate() {
    if (videoEl) onTimeUpdate(videoEl.currentTime);
  }

  $effect(() => {
    if (streamUrl && videoEl) setupPlayer();
  });

  onDestroy(() => destroyPlayer());
</script>

<!-- svelte-ignore a11y_media_has_caption -->
<video
  bind:this={videoEl}
  ontimeupdate={handleTimeUpdate}
  autoplay
  playsinline
  preload="auto"
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
