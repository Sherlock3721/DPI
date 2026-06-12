<script lang="ts">
  import { run } from 'svelte/legacy';

  import { createEventDispatcher } from "svelte";
  import { ChevronDown, ChevronUp } from "lucide-svelte";
  import type { ComponentType } from "svelte";

  interface Props {
    value: string | number;
    options?: {
    value: string | number;
    label: string;
    color?: string;
    cssStyle?: string;
    icon?: ComponentType;
  }[];
    placeholder?: string;
    cssStyle?: string;
  }

  let {
    value = $bindable(),
    options = [],
    placeholder = "Vyberte...",
    cssStyle = ""
  }: Props = $props();

  let isOpen = $state(false);
  let wrapperRef: HTMLDivElement = $state()!;

  const dispatch = createEventDispatcher();

  let dropdownEl: HTMLUListElement = $state()!;

  function portal(node: HTMLElement) {
    document.body.appendChild(node);

    const updatePos = () => updatePosition();
    window.addEventListener("scroll", updatePos, true);
    window.addEventListener("resize", updatePos);

    return {
      destroy() {
        window.removeEventListener("scroll", updatePos, true);
        window.removeEventListener("resize", updatePos);
        if (node.parentNode) {
          node.parentNode.removeChild(node);
        }
      },
    };
  }

  function updatePosition() {
    if (isOpen && wrapperRef && dropdownEl) {
      const rect = wrapperRef.getBoundingClientRect();
      const spaceBelow = window.innerHeight - rect.bottom;
      const dropdownHeight = dropdownEl.offsetHeight || 240;

      dropdownEl.style.position = "fixed";
      dropdownEl.style.left = `${rect.left}px`;
      dropdownEl.style.width = `${rect.width}px`;

      if (spaceBelow < dropdownHeight && rect.top > dropdownHeight) {
        dropdownEl.style.top = "auto";
        dropdownEl.style.bottom = `${window.innerHeight - rect.top + 4}px`;
        dropdownEl.style.marginTop = "0";
        dropdownEl.style.marginBottom = "4px";
      } else {
        dropdownEl.style.top = `${rect.bottom + 4}px`;
        dropdownEl.style.bottom = "auto";
        dropdownEl.style.marginTop = "0";
        dropdownEl.style.marginBottom = "0";
      }
    }
  }

  run(() => {
    if (isOpen) {
      setTimeout(updatePosition, 0);
    }
  });

  function toggle() {
    isOpen = !isOpen;
  }

  function selectOption(option: any) {
    value = option.value;
    isOpen = false;
    dispatch("change", { value });
  }

  function handleClickOutside(event: MouseEvent) {
    if (wrapperRef && !wrapperRef.contains(event.target as Node)) {
      isOpen = false;
    }
  }

  let selectedOption = $derived(options.find((o) => o.value === value) || null);
</script>

<svelte:window onclick={handleClickOutside} />

<div bind:this={wrapperRef} class="relative w-full text-[11px]" style={cssStyle}>
  <button
    type="button"
    onclick={toggle}
    class="w-full flex items-center justify-between input-premium"
  >
    <div class="flex items-center gap-2 overflow-hidden">
      {#if selectedOption}
        {#if selectedOption.color}
          <div
            class="w-4 h-4 shrink-0 rounded-sm border border-slate-700/50 shadow-inner"
            style={selectedOption.cssStyle
              ? selectedOption.cssStyle
              : `background-color: ${selectedOption.color}`}
          ></div>
        {/if}
        {#if selectedOption.icon}
          <div class="w-4 h-4 shrink-0 flex items-center justify-center text-slate-400">
            <selectedOption.icon class="w-4 h-4" />
          </div>
        {/if}
        <span class="truncate">{selectedOption.label}</span>
      {:else}
        <span class="text-slate-500 truncate">{placeholder}</span>
      {/if}
    </div>
    {#if isOpen}
      <ChevronUp class="w-3.5 h-3.5 text-slate-500 shrink-0" />
    {:else}
      <ChevronDown class="w-3.5 h-3.5 text-slate-500 shrink-0" />
    {/if}
  </button>

  {#if isOpen}
    <ul
      use:portal
      bind:this={dropdownEl}
      class="fixed z-9999 max-h-60 overflow-y-auto bg-slate-800 border border-slate-700 rounded-sm shadow-xl text-slate-300 divide-y divide-slate-700/50 custom-scrollbar text-[11px]"
    >
      {#each options as option}
        <li>
          <button
            type="button"
            onclick={() => selectOption(option)}
            class="w-full flex items-center gap-2 px-2 py-1.5 hover:bg-labaccent/20 hover:text-white transition-colors text-left"
          >
            {#if option.color}
              <div
                class="w-4 h-4 shrink-0 rounded-sm border border-slate-700/50 shadow-inner"
                style={option.cssStyle ? option.cssStyle : `background-color: ${option.color}`}
              ></div>
            {/if}
            {#if option.icon}
              <div class="w-4 h-4 shrink-0 flex items-center justify-center text-slate-400">
                <option.icon class="w-4 h-4" />
              </div>
            {/if}
            <span class="truncate">{option.label}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .custom-scrollbar::-webkit-scrollbar {
    width: 4px;
  }
  .custom-scrollbar::-webkit-scrollbar-track {
    background: rgba(15, 23, 42, 0.5);
  }
  .custom-scrollbar::-webkit-scrollbar-thumb {
    background: rgba(51, 65, 85, 0.8);
    border-radius: 4px;
  }
</style>
