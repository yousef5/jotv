import type { Snapshot } from '@sveltejs/kit';

// Pages scroll inside the layout's <main>, not the window, so SvelteKit's
// built-in scroll restoration doesn't apply. Pages export this snapshot to get
// their position back on browser back/forward.

function mainEl(): HTMLElement | null {
  return document.querySelector('main');
}

/**
 * Scrolls <main> to `y`, retrying while async content is still growing the page.
 * Stops as soon as the user scrolls on their own.
 */
export function restoreMainScroll(y: number, timeoutMs = 2500): void {
  const main = mainEl();
  if (!main || y <= 0) return;

  let cancelled = false;
  const cancel = () => (cancelled = true);
  const opts = { once: true, passive: true } as const;
  main.addEventListener('wheel', cancel, opts);
  main.addEventListener('touchstart', cancel, opts);
  window.addEventListener('keydown', cancel, { once: true });

  const start = performance.now();
  const attempt = () => {
    if (cancelled) return;
    main.scrollTop = y;
    const reached = Math.abs(main.scrollTop - y) <= 2;
    if (!reached && performance.now() - start < timeoutMs) requestAnimationFrame(attempt);
    else {
      main.removeEventListener('wheel', cancel);
      main.removeEventListener('touchstart', cancel);
      window.removeEventListener('keydown', cancel);
    }
  };
  attempt();
}

export const mainScrollSnapshot: Snapshot<number> = {
  capture: () => mainEl()?.scrollTop ?? 0,
  restore: (y) => restoreMainScroll(y),
};
