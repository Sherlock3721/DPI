<script lang="ts">
  import { ChevronRight } from "lucide-svelte";
  import { createEventDispatcher } from "svelte";

  export let title = "";
  export let isOpen = false;
  export let headerClass = "text-labaccent";
  export let containerClass = "border-slate-800 bg-slate-950/20";

  const dispatch = createEventDispatcher();

  function toggle() {
    isOpen = !isOpen;
    dispatch("toggle", isOpen);
  }
</script>

<div
  class="flex flex-col border rounded-lg overflow-hidden transition-all duration-200 {containerClass}"
>
  <!-- TOGGLE HEADER -->
  <button
    type="button"
    on:click={toggle}
    class="w-full flex items-center justify-between px-3 py-2.5 text-left font-bold text-xs hover:bg-slate-900/40 transition-colors focus:outline-none {headerClass}"
  >
    <span>{title}</span>
    <ChevronRight
      class="w-4 h-4 transform transition-transform duration-200 {isOpen
        ? 'rotate-90 text-slate-400'
        : 'text-slate-500'}"
    />
  </button>

  <!-- CONTENT CONTAINER -->
  {#if isOpen}
    <div class="px-3 pb-3 border-t border-slate-900/50 bg-slate-950/10">
      <slot />
    </div>
  {/if}
</div>
