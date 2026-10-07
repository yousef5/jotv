<script lang="ts" module>
  export interface DropdownOption {
    value: string;
    label: string;
    /** Indented under the option before it (sub-categories) */
    sub?: boolean;
    color?: string;
  }
</script>

<script lang="ts">
  import { tick } from 'svelte';
  import { fly } from 'svelte/transition';

  // A dark menu in place of <select>: WebKitGTK draws native selects in the
  // light GTK theme, and they can't show colour dots or nesting.

  let {
    value = '',
    options,
    onchange,
    label,
    placeholder = 'Choose…',
    /** Fixed text on the button (e.g. "Move to…") instead of the current value */
    buttonText,
    disabled = false,
    up = false,
    full = false,
  }: {
    value?: string;
    options: DropdownOption[];
    onchange: (value: string) => void;
    label: string;
    placeholder?: string;
    buttonText?: string;
    disabled?: boolean;
    /** Open above the button (for bars at the bottom of the screen) */
    up?: boolean;
    full?: boolean;
  } = $props();

  let open = $state(false);
  let active = $state(0);
  let root = $state<HTMLDivElement | undefined>();
  let list = $state<HTMLUListElement | undefined>();
  let current = $derived(options.find((o) => o.value === value));
  const uid = `dd-${Math.random().toString(36).slice(2, 8)}`;

  async function show() {
    if (disabled) return;
    open = true;
    active = Math.max(0, options.findIndex((o) => o.value === value));
    await tick();
    list?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  }

  function pick(o: DropdownOption) {
    open = false;
    if (o.value !== value || buttonText) onchange(o.value);
    root?.querySelector('button')?.focus();
  }

  function onKey(e: KeyboardEvent) {
    if (!open) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        show();
      }
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      open = false;
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      active = Math.min(options.length - 1, active + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      active = Math.max(0, active - 1);
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      if (options[active]) pick(options[active]);
    } else if (e.key === 'Tab') {
      open = false;
    }
    tick().then(() => list?.children[active]?.scrollIntoView({ block: 'nearest' }));
  }

  function outside(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onpointerdown={outside} />

<div class="dd" class:full bind:this={root}>
  <button
    type="button"
    class="trigger"
    class:open
    onclick={() => (open ? (open = false) : show())}
    onkeydown={onKey}
    {disabled}
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls={uid}
    aria-label={label}
  >
    {#if !buttonText && current?.color}<span class="dot" style:background={current.color}></span>{/if}
    <span class="val" dir="auto">{buttonText ?? current?.label ?? placeholder}</span>
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>
  </button>
  {#if open}
    <ul class="menu" class:up id={uid} role="listbox" aria-label={label} bind:this={list} transition:fly={{ y: up ? 6 : -6, duration: 140 }}>
      {#each options as o, i (o.value)}
        <li
          role="option"
          aria-selected={o.value === value && !buttonText}
          class:active={i === active}
          class:sub={o.sub}
          onpointerenter={() => (active = i)}
          onclick={() => pick(o)}
          onkeydown={() => {}}
        >
          {#if o.color}<span class="dot" style:background={o.color}></span>{/if}
          <span class="val" dir="auto">{o.label}</span>
          {#if o.value === value && !buttonText}
            <svg class="tick" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.8" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  /* Arabic names stay next to their dot, not across the menu */
  [dir="auto"] { text-align: left; }
  .dd { position: relative; display: inline-flex; min-width: 0; }
  .dd.full { display: flex; width: 100%; }
  .trigger {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-width: 0;
    height: var(--dd-h, 40px);
    padding: 0 10px 0 12px;
    border-radius: 8px;
    background: var(--dd-bg, var(--color-surface));
    box-shadow: inset 0 0 0 1px var(--dd-border, var(--color-border));
    color: var(--color-text);
    font-size: 0.8125rem;
    font-weight: 600;
    text-align: start;
    transition: background 120ms var(--ease-out), box-shadow 120ms var(--ease-out);
  }
  .trigger:hover { background: var(--color-hover); }
  .trigger.open, .trigger:focus-visible { outline: none; box-shadow: inset 0 0 0 2px var(--color-text-muted); }
  .trigger:disabled { opacity: 0.45; cursor: default; }
  .trigger .val { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .trigger > svg { flex: none; width: 14px; height: 14px; color: var(--color-text-muted); transition: transform 150ms var(--ease-out); }
  .trigger.open > svg { transform: rotate(180deg); }

  .menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 80;
    min-width: max(100%, 190px);
    max-width: 320px;
    max-height: 320px;
    overflow-y: auto;
    margin: 0;
    padding: 5px;
    list-style: none;
    border-radius: 10px;
    background: oklch(0.22 0.005 25);
    box-shadow: 0 22px 48px -12px oklch(0 0 0 / 0.85), 0 0 0 1px oklch(1 0 0 / 0.1);
    scrollbar-width: thin;
  }
  .menu.up { top: auto; bottom: calc(100% + 8px); }
  li {
    display: flex;
    align-items: center;
    gap: 9px;
    min-height: 34px;
    padding: 0 10px;
    border-radius: 6px;
    font-size: 0.8125rem;
    font-weight: 600;
    color: oklch(0.88 0.004 25);
    cursor: pointer;
  }
  li.sub { padding-left: 28px; font-weight: 500; }
  li.active { background: oklch(1 0 0 / 0.08); color: var(--color-text); }
  li[aria-selected='true'] { color: var(--color-text); }
  li .val { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tick { flex: none; width: 15px; height: 15px; color: var(--color-accent-soft); }
  .dot { flex: none; width: 9px; height: 9px; border-radius: 50%; }
</style>
