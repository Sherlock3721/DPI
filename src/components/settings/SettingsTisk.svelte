<script lang="ts">
  import CustomSelect from "../CustomSelect.svelte";

  interface Props {
    settings: any;
  }

  let { settings = $bindable() }: Props = $props();
</script>

<div class="flex flex-col gap-5">
  <span class="font-bold text-xs text-slate-300 pb-1 border-b border-slate-800"
    >Nastavení tisku</span
  >

  <!-- POŘADÍ TISKU -->
  <div class="flex flex-col gap-3">
    <div class="flex items-center gap-2">
      <span class="w-2 h-2 rounded-full bg-sky-500"></span>
      <span class="text-xs font-bold text-slate-300 uppercase tracking-wider">Pořadí tisku</span>
    </div>
    <div class="grid grid-cols-1 gap-2.5 text-xs pl-4">
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Pořadí tisku</div>
          <div class="text-[10px] text-slate-500 mt-0.5">
            Substrát po substrátu, nebo napřed vše jednou tryskou napříč substráty
          </div>
        </div>
        <div class="col-span-2">
          <CustomSelect
            bind:value={settings.print_order}
            options={[
              { value: "nozzle_major", label: "Nejprve vytisknout vše jednou tryskou" },
              { value: "substrate_major", label: "Nejprve vytisknout vše na substrát" },
            ]}
          />
        </div>
      </div>
    </div>
  </div>

  <div class="border-b border-slate-800/60"></div>

  <!-- GENEROVÁNÍ A TISK TRASY -->
  <div class="flex flex-col gap-3">
    <div class="flex items-center gap-2">
      <span class="w-2 h-2 rounded-full bg-teal-500"></span>
      <span class="text-xs font-bold text-slate-300 uppercase tracking-wider"
        >Generování &amp; tisk trasy</span
      >
    </div>
    <div class="grid grid-cols-1 gap-2.5 text-xs pl-4">
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Generování &amp; tisk trasy</div>
          <div class="text-[10px] text-slate-500 mt-0.5">
            Strategie přejezdů mezi objekty na substrátu
          </div>
        </div>
        <div class="col-span-2">
          <CustomSelect
            bind:value={settings.route_mode}
            options={[
              { value: "fastest", label: "Nejrychleji" },
              { value: "cross_allowed", label: "Minimum přejezdů (s přejezdem nad vytištěnou trasou)" },
              { value: "no_cross", label: "Minimum přejezdů (bez přejezdu nad vytištěnou trasou)" },
            ]}
          />
        </div>
      </div>
    </div>
  </div>
</div>
