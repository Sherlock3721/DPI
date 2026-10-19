<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Minus, Plus } from "lucide-svelte";

  interface Props {
    value: number | string;
    step?: number;
    min?: number;
    max?: number;
    placeholder?: string;
    class?: string;
    /** Jednotka uvnitř pole — zůstává vidět i v úzkém panelu (číslo se zúží dřív). */
    unit?: string;
    /** Zvláštní hodnota pod `min` zobrazená textem (např. -1 = „Vypnuto“).
     *  Krok dolů z `min` vede na ni, krok nahoru z ní na `min`. */
    special?: { value: number; label: string };
  }

  let {
    value = $bindable(),
    step = 1,
    min = -Infinity,
    max = Infinity,
    placeholder = "",
    class: className = "",
    unit = "",
    special = undefined
  }: Props = $props();

  const dispatch = createEventDispatcher();

  let isSpecial = $derived(special != null && Number(value) === special.value);
  let display = $derived(isSpecial ? special!.label : String(value ?? ""));

  function current(): number {
    const v = typeof value === "string" ? parseFloat(value) : value;
    return isNaN(v) ? 0 : v;
  }

  function decrement() {
    if (isSpecial) return;
    const next = current() - step;
    if (special && next < min - 1e-9) value = special.value;
    else value = parseFloat(Math.max(min, next).toPrecision(12));
    dispatch("input", value);
  }

  function increment() {
    const next = isSpecial ? (Number.isFinite(min) ? min : 0) : current() + step;
    value = parseFloat(Math.min(max, next).toPrecision(12));
    dispatch("input", value);
  }

  function handleInput(e: Event) {
    const val = parseFloat((e.target as HTMLInputElement).value.replace(",", "."));
    // Se zvláštní hodnotou se číslo pod minimem (rozepsané „4“ z „45“) převezme až při potvrzení
    if (!isNaN(val) && (!special || (val >= min && val <= max))) {
      value = val;
      dispatch("input", value);
    }
  }

  /** Potvrzení textu (Enter, opuštění pole): pod minimem → `min`, nula a méně → zvláštní hodnota. */
  function commit(el: HTMLInputElement) {
    const val = parseFloat(el.value.replace(",", "."));
    if (!isNaN(val) && special) {
      const next = val < min ? (val <= 0 ? special.value : min) : Math.min(max, val);
      if (next !== Number(value)) {
        value = next;
        dispatch("input", value);
      }
    }
    el.value = display;
  }
</script>

<div
  class="@container flex items-center {className} h-full bg-slate-950 border border-slate-700/50 rounded-sm overflow-hidden"
>
  {#if special}
    <!-- Textové pole: ukáže i popis zvláštní hodnoty místo čísla -->
    <input
      type="text"
      inputmode="decimal"
      value={display}
      oninput={handleInput}
      onkeydown={(e) => {
        if (e.key === "ArrowUp" || e.key === "ArrowDown") {
          e.preventDefault();
          if (e.key === "ArrowUp") increment();
          else decrement();
          e.currentTarget.value = display;
        } else if (e.key === "Enter") {
          commit(e.currentTarget);
        }
      }}
      onfocus={(e) => isSpecial && e.currentTarget.select()}
      onblur={(e) => commit(e.currentTarget)}
      {placeholder}
      class="flex-1 bg-transparent border-none text-left pl-2 pr-1 outline-hidden w-full min-w-0 py-0.5 text-[11px] {isSpecial
        ? 'text-slate-400 italic'
        : 'text-slate-200'}"
    />
  {:else}
    <input
      type="number"
      {min}
      {max}
      {step}
      bind:value
      oninput={handleInput}
      {placeholder}
      class="flex-1 bg-transparent border-none text-left pl-2 pr-1 outline-hidden text-slate-200 w-full min-w-0 py-0.5 text-[11px]"
      style="-moz-appearance: textfield;"
    />
  {/if}

  {#if unit && !isSpecial}
    <span class="shrink-0 whitespace-nowrap pr-1.5 text-[10px] text-slate-400 select-none">{unit}</span>
  {/if}

  <!-- V úzkém poli se tlačítka skryjí, aby zůstalo místo pro číslo a jednotku -->
  <div
    class="{unit.length > 3
      ? 'hidden @min-[9rem]:flex'
      : unit
        ? 'hidden @min-[7rem]:flex'
        : 'hidden @min-[4.5rem]:flex'} h-full border-l border-slate-700/50 shrink-0"
  >
    <button
      type="button"
      onclick={decrement}
      class="h-full px-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 transition-colors border-r border-slate-700/50 flex items-center justify-center"
    >
      <Minus class="w-3 h-3" />
    </button>
    <button
      type="button"
      onclick={increment}
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
