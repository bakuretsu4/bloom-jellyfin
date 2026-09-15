<script lang="ts" module>
  let count = 0;
</script>

<script lang="ts" generics="T extends string">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";

  type Option = { value: T; label: string; detail?: string | null };

  let {
    value,
    options,
    label,
    onChange,
    look = "field",
  }: {
    value: T;
    options: Option[];
    /** The accessible name: what is being chosen ("Sort by", "Season"). */
    label: string;
    onChange: (value: T) => void;
    /** "field" is a bordered control; "heading" reads as a section title that opens a list. */
    look?: "field" | "heading";
  } = $props();

  // The platform's own list can't be styled on WebKitGTK (GTK draws it), so this is a listbox.
  const uid = `bloom-select-${++count}`;
  let open = $state(false);
  let active = $state(0);
  let root = $state<HTMLDivElement>();
  let trigger = $state<HTMLButtonElement>();
  let list = $state<HTMLUListElement>();

  let current = $derived(options.find((o) => o.value === value));

  function show() {
    active = Math.max(0, options.findIndex((o) => o.value === value));
    open = true;
    tick().then(() => {
      list?.focus();
      reveal();
    });
  }

  function close(returnFocus: boolean) {
    open = false;
    if (returnFocus) trigger?.focus();
  }

  function choose(index: number) {
    const option = options[index];
    close(true);
    if (option && option.value !== value) onChange(option.value);
  }

  function reveal() {
    list?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function onTriggerKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      show();
    }
  }

  function onListKey(e: KeyboardEvent) {
    switch (e.key) {
      case "ArrowDown":
        active = Math.min(options.length - 1, active + 1);
        break;
      case "ArrowUp":
        active = Math.max(0, active - 1);
        break;
      case "Home":
        active = 0;
        break;
      case "End":
        active = options.length - 1;
        break;
      case "Enter":
      case " ":
        choose(active);
        break;
      case "Escape":
        close(true);
        break;
      case "Tab":
        close(false);
        return;
      default: {
        // Type a letter to jump to the next option starting with it.
        if (e.key.length !== 1 || e.ctrlKey || e.metaKey || e.altKey) return;
        const key = e.key.toLowerCase();
        const n = options.length;
        for (let step = 1; step <= n; step++) {
          const i = (active + step) % n;
          if (options[i].label.toLowerCase().startsWith(key)) {
            active = i;
            break;
          }
        }
      }
    }
    e.preventDefault();
    e.stopPropagation();
    tick().then(reveal);
  }

  $effect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (!root?.contains(e.target as Node)) close(false);
    };
    document.addEventListener("pointerdown", onPointer);
    return () => document.removeEventListener("pointerdown", onPointer);
  });
</script>

<div class="dropdown" class:is-heading={look === "heading"} class:is-open={open} bind:this={root}>
  <button
    class="trigger"
    type="button"
    bind:this={trigger}
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls={open ? `${uid}-list` : undefined}
    aria-label="{label}: {current?.label ?? ''}"
    onclick={() => (open ? close(true) : show())}
    onkeydown={onTriggerKey}
  >
    <span class="value">{current?.label ?? ""}</span>
    <span class="caret" aria-hidden="true"></span>
  </button>

  {#if open}
    <ul
      class="list"
      id="{uid}-list"
      role="listbox"
      tabindex="-1"
      aria-label={label}
      aria-activedescendant="{uid}-{active}"
      bind:this={list}
      onkeydown={onListKey}
    >
      {#each options as option, i (option.value)}
        <!-- svelte-ignore a11y_click_events_have_key_events: the listbox handles keys -->
        <li
          id="{uid}-{i}"
          data-index={i}
          role="option"
          aria-selected={option.value === value}
          class:is-active={i === active}
          onpointermove={() => (active = i)}
          onclick={() => choose(i)}
        >
          <span class="label">{option.label}</span>
          {#if option.detail}<span class="detail">{option.detail}</span>{/if}
          <span class="mark"><Icon name="check" size={14} /></span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  /* Never squeezed by its neighbours in a heading: the chosen option is the point of it. */
  .dropdown {
    position: relative;
    display: inline-block;
    flex: none;
  }
  .trigger {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 11px;
    border: 1px solid var(--line);
    border-radius: var(--r-ctl);
    background: var(--surface);
    color: var(--ink);
    font: inherit;
    font-size: 13.5px;
    cursor: pointer;
    transition: background 0.16s var(--ease), border-color 0.16s var(--ease);
  }
  .trigger:hover {
    background: var(--surface-2);
  }
  .is-open .trigger {
    border-color: color-mix(in oklab, var(--accent) 42%, var(--line));
  }
  /* Always whole: a picker reading "Seas..." hides the one thing it's there to show. */
  .value {
    white-space: nowrap;
  }
  .caret {
    flex: none;
    width: 0;
    height: 0;
    margin-top: 3px;
    border: 4px solid transparent;
    border-top-color: var(--ink-3);
    transform-origin: 50% 25%;
    transition: transform 0.2s var(--ease);
  }
  .is-open .caret {
    transform: rotate(180deg);
  }

  /* A section title that happens to open a list: no box until hovered. */
  .is-heading .trigger {
    height: auto;
    margin-left: -8px;
    padding: 4px 8px;
    border-color: transparent;
    background: none;
    font-size: 16px;
    font-weight: 600;
    letter-spacing: -0.005em;
  }
  .is-heading .trigger:hover,
  .is-heading.is-open .trigger {
    background: var(--surface);
    border-color: transparent;
  }

  .list {
    position: absolute;
    left: 0;
    top: calc(100% + 6px);
    z-index: 30;
    min-width: max(100%, 200px);
    max-height: min(340px, 60vh);
    overflow-y: auto;
    margin: 0;
    padding: 6px;
    list-style: none;
    background: var(--raise);
    border: 1px solid var(--line);
    border-radius: var(--r);
    box-shadow: var(--shadow);
    outline: none;
    animation: pop 0.2s var(--ease) both;
  }
  .is-heading .list {
    left: -8px;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto 14px;
    align-items: center;
    gap: 14px;
    padding: 7px 8px;
    border-radius: var(--r-ctl);
    color: var(--ink-2);
    font-size: 13.5px;
    font-weight: 400;
    white-space: nowrap;
    cursor: pointer;
  }
  li.is-active {
    background: var(--surface-2);
    color: var(--ink);
  }
  li[aria-selected="true"] {
    color: var(--ink);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .detail {
    font-size: 12px;
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  .mark {
    display: grid;
    color: var(--accent);
    opacity: 0;
  }
  li[aria-selected="true"] .mark {
    opacity: 1;
  }
</style>
