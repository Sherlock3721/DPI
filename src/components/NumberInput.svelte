<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Minus, Plus } from "lucide-svelte";

  export let value: number | string;
  export let step: number = 1;
  export let min: number = -Infinity;
  export let max: number = Infinity;
  export let placeholder: string = "";
  let className: string = "";
  export { className as class };

  const dispatch = createEventDispatcher();

  function decrement() {
    let current = typeof value === "string" ? parseFloat(value) : value;
    if (isNaN(current)) current = 0;
    let next = Math.max(min, current - step);
    value = parseFloat(next.toPrecision(12));
    dispatch("input", value);
  }

  function increment() {
    let current = typeof value === "string" ? parseFloat(value) : value;
    if (isNaN(current)) current = 0;
    let next = Math.min(max, current + step);
    value = parseFloat(next.toPrecision(12));
    dispatch("input", value);
  }

  function handleInput(e: Event) {
    const val = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(val)) {
      value = val;
      dispatch("input", value);
    }
  }
</script>

<div
  class="flex items-center {className} h-full bg-slate-950 border border-slate-700/50 rounded overflow-hidden"
>
  <input
    type="number"
    {min}
    {max}
    {step}
    bind:value
    on:input={handleInput}
    {placeholder}
    class="flex-1 bg-transparent border-none text-left pl-2 pr-1 outline-none text-slate-200 w-full min-w-0 py-0.5 text-[11px]"
    style="-moz-appearance: textfield;"
  />

  <div class="flex h-full border-l border-slate-700/50 shrink-0">
    <button
      type="button"
      on:click={decrement}
      class="h-full px-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 transition-colors border-r border-slate-700/50 flex items-center justify-center"
    >
      <Minus class="w-3 h-3" />
    </button>
    <button
      type="button"
      on:click={increment}
      class="h-full px-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 transition-colors flex items-center justify-center"
    >
      <Plus class="w-3 h-3" />
    </button>
  </div>
</div>

<style>
  /* Hide native browser spinners */
  input[type="number"]::-webkit-inner-spin-button,
  input[type="number"]::-webkit-outer-spin-button {
    -webkit-appearance: none;
    margin: 0;
  }
</style>
