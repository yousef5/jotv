/**
 * Fades driven frame by frame from script instead of CSS.
 *
 * WebKitGTK (the Linux webview) glitches for one frame whenever a GPU-run
 * opacity/transform animation or transition finishes: the element flashes back
 * to its old state. Setting `style.opacity` from requestAnimationFrame never
 * hands an animation off, so it can't blink. Use these for anything that fades.
 */

const easeOut = (t: number) => 1 - Math.pow(1 - t, 3);

/** Svelte transition (`in:jsFade`, `out:jsFade`, `transition:jsFade`) */
export function jsFade(node: HTMLElement, { duration = 300, delay = 0 } = {}) {
  return {
    duration,
    delay,
    easing: easeOut,
    tick: (t: number) => {
      node.style.opacity = String(t);
    },
  };
}

/** Action: `use:fadeTo={visible}` animates the element's opacity to 1 or 0 */
export function fadeTo(node: HTMLElement, visible: boolean, duration = 900) {
  let current = visible ? 1 : 0;
  let raf = 0;
  node.style.opacity = String(current);

  function animate(to: number) {
    cancelAnimationFrame(raf);
    const from = current;
    if (from === to) return;
    const t0 = performance.now();
    const span = duration * Math.abs(to - from);
    const step = (now: number) => {
      const k = Math.min(1, (now - t0) / span);
      current = from + (to - from) * easeOut(k);
      node.style.opacity = String(current);
      if (k < 1) raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
  }

  return {
    update: (v: boolean) => animate(v ? 1 : 0),
    destroy: () => cancelAnimationFrame(raf),
  };
}

/** Hand the element back to its stylesheet once a move has finished */
function clear(node: HTMLElement) {
  node.style.removeProperty('opacity');
  node.style.removeProperty('transform');
}

/**
 * Svelte transition: slide by (x, y) px while fading, like svelte's `fly`, but set
 * from `tick` each frame. Composes with any transform the element already has.
 */
export function jsFly(node: HTMLElement, { x = 0, y = 0, duration = 200, delay = 0 } = {}) {
  const base = getComputedStyle(node).transform;
  const rest = base === 'none' ? '' : ` ${base}`;
  return {
    duration,
    delay,
    easing: easeOut,
    tick: (t: number) => {
      if (t === 1) return clear(node);
      const u = 1 - t;
      node.style.opacity = String(t);
      node.style.transform = `translate(${x * u}px, ${y * u}px)${rest}`;
    },
  };
}

/** Svelte transition: scale from `start` while fading, like svelte's `scale`, from `tick` */
export function jsScale(node: HTMLElement, { start = 0.95, duration = 200, delay = 0 } = {}) {
  const base = getComputedStyle(node).transform;
  const rest = base === 'none' ? '' : ` ${base}`;
  return {
    duration,
    delay,
    easing: easeOut,
    tick: (t: number) => {
      if (t === 1) return clear(node);
      node.style.opacity = String(t);
      node.style.transform = `scale(${start + (1 - start) * t})${rest}`;
    },
  };
}

/**
 * Svelte transition: grow/shrink height like svelte's `slide`, from `tick`.
 * Height, padding and margin are layout properties, never GPU-composited.
 */
export function jsSlide(node: HTMLElement, { duration = 200, delay = 0 } = {}) {
  const cs = getComputedStyle(node);
  const h = parseFloat(cs.height);
  const pt = parseFloat(cs.paddingTop), pb = parseFloat(cs.paddingBottom);
  const mt = parseFloat(cs.marginTop), mb = parseFloat(cs.marginBottom);
  const bt = parseFloat(cs.borderTopWidth), bb = parseFloat(cs.borderBottomWidth);
  return {
    duration,
    delay,
    easing: easeOut,
    tick: (t: number) => {
      const s = node.style;
      if (t === 1) {
        for (const p of ['overflow', 'height', 'padding-top', 'padding-bottom', 'margin-top', 'margin-bottom', 'border-top-width', 'border-bottom-width', 'opacity'])
          s.removeProperty(p);
        return;
      }
      s.overflow = 'hidden';
      s.opacity = String(Math.min(t * 20, 1));
      s.height = `${t * h}px`;
      s.paddingTop = `${t * pt}px`;
      s.paddingBottom = `${t * pb}px`;
      s.marginTop = `${t * mt}px`;
      s.marginBottom = `${t * mb}px`;
      s.borderTopWidth = `${t * bt}px`;
      s.borderBottomWidth = `${t * bb}px`;
    },
  };
}

/** `animate:jsFlip`: like svelte's `flip` (reorders glide into place), from `tick` */
export function jsFlip(
  node: Element,
  { from, to }: { from: DOMRect; to: DOMRect },
  { duration = 220, delay = 0 } = {},
) {
  const el = node as HTMLElement;
  const base = getComputedStyle(el).transform;
  const rest = base === 'none' ? '' : `${base} `;
  const dx = from.left - to.left;
  const dy = from.top - to.top;
  return {
    duration,
    delay,
    easing: easeOut,
    tick: (t: number, u: number) => {
      if (t === 1) el.style.removeProperty('transform');
      else el.style.transform = `${rest}translate(${u * dx}px, ${u * dy}px)`;
    },
  };
}
