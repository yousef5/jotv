<script lang="ts">
  // Tags as chips: type and press Enter or comma, Backspace removes the last
  // one, and tags already used in the library are suggested as you type.

  let {
    tags = $bindable([]),
    suggestions = [],
    placeholder = 'Add tags',
  }: { tags?: string[]; suggestions?: string[]; placeholder?: string } = $props();

  let text = $state('');
  let focused = $state(false);

  function norm(s: string): string {
    return s.toLowerCase().replace(/[ً-ٰٟ]/g, '').replace(/[أإآ]/g, 'ا').replace(/ى/g, 'ي').replace(/ة/g, 'ه');
  }

  let shown = $derived.by(() => {
    const have = new Set(tags.map((t) => t.toLowerCase()));
    const q = norm(text.trim().replace(/^#/, ''));
    return suggestions.filter((t) => !have.has(t.toLowerCase()) && (!q || norm(t).includes(q))).slice(0, q ? 8 : 10);
  });

  function add(raw: string) {
    const next = [...tags];
    for (const t of raw.split(/[,،\n]/).map((x) => x.trim().replace(/^#/, '').replace(/_/g, ' ')).filter(Boolean)) {
      if (!next.some((x) => x.toLowerCase() === t.toLowerCase())) next.push(t);
    }
    tags = next;
    text = '';
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',') {
      if (!text.trim()) return;
      e.preventDefault();
      e.stopPropagation();
      add(text);
    } else if (e.key === 'Backspace' && !text && tags.length) {
      tags = tags.slice(0, -1);
    }
  }
</script>

<div class="wrap">
  <div class="box" class:focused>
    {#each tags as t (t)}
      <span class="chip">
        <span dir="auto">#{t}</span>
        <button type="button" onclick={() => (tags = tags.filter((x) => x !== t))} aria-label="Remove tag {t}">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"><line x1="7" y1="7" x2="17" y2="17"/><line x1="17" y1="7" x2="7" y2="17"/></svg>
        </button>
      </span>
    {/each}
    <input
      bind:value={text}
      onkeydown={onKey}
      onfocus={() => (focused = true)}
      onblur={() => {
        focused = false;
        if (text.trim()) add(text);
      }}
      placeholder={tags.length ? 'Add more…' : placeholder}
      dir="auto"
      aria-label="Add a tag"
    />
  </div>
  {#if shown.length}
    <div class="suggest" aria-label="Suggested tags">
      {#each shown as t (t)}
        <button type="button" onmousedown={(e) => e.preventDefault()} onclick={() => add(t)}>+ {t}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .wrap { display: flex; flex-direction: column; gap: 7px; }
  .box {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    min-height: 40px;
    padding: 5px 6px;
    border-radius: 8px;
    background: var(--color-base);
    box-shadow: inset 0 0 0 1px var(--color-border);
  }
  .box.focused { box-shadow: inset 0 0 0 2px var(--color-text-muted); }
  .box input {
    flex: 1;
    min-width: 120px;
    height: 28px;
    padding: 0 6px;
    border: none;
    background: none;
    color: var(--color-text);
    font-size: 0.875rem;
    font-weight: 600;
    outline: none;
    text-align: left;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 28px;
    padding: 0 4px 0 10px;
    border-radius: 14px;
    background: oklch(1 0 0 / 0.1);
    font-size: 0.8125rem;
    font-weight: 600;
  }
  .chip button { width: 20px; height: 20px; display: grid; place-items: center; border-radius: 50%; background: none; color: var(--color-text-muted); }
  .chip button:hover { background: oklch(1 0 0 / 0.16); color: var(--color-text); }
  .chip svg { width: 10px; height: 10px; }
  .suggest { display: flex; flex-wrap: wrap; gap: 5px; }
  .suggest button {
    height: 26px;
    padding: 0 10px;
    border-radius: 13px;
    background: none;
    box-shadow: inset 0 0 0 1px oklch(1 0 0 / 0.14);
    color: var(--color-text-muted);
    font-size: 0.75rem;
    font-weight: 600;
  }
  .suggest button:hover { color: var(--color-text); background: oklch(1 0 0 / 0.06); }
</style>
