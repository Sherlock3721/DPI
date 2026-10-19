<script lang="ts">
  interface Props {
    settings: any;
  }

  let { settings = $bindable() }: Props = $props();
</script>

<div class="flex flex-col gap-4 text-xs h-full">
  <span class="font-bold text-xs text-slate-300 pb-1 border-b border-slate-800"
    >Inicializační a ukončovací sekvence G-kódu</span
  >
  <div class="flex flex-col gap-1">
    <span class="text-slate-400 font-bold uppercase text-[9px]">Start G-code</span>
    <textarea
      bind:value={settings.start_gcode}
      rows="4"
      class="input-premium font-mono text-[10px] w-full resize-y min-h-[80px]"
    ></textarea>
  </div>
  <div class="flex flex-col gap-1">
    <span class="text-slate-400 font-bold uppercase text-[9px]">End G-code</span>
    <textarea
      bind:value={settings.end_gcode}
      rows="4"
      class="input-premium font-mono text-[10px] w-full resize-y min-h-[80px]"
    ></textarea>
  </div>
  <div class="flex flex-col gap-1">
    <span class="text-slate-400 font-bold uppercase text-[9px]">Loop Start G-code</span>
    <textarea
      bind:value={settings.loop_start_gcode}
      rows="2"
      class="input-premium font-mono text-[10px] w-full resize-y min-h-[50px]"
    ></textarea>
  </div>
  <div class="flex flex-col gap-1">
    <span class="text-slate-400 font-bold uppercase text-[9px]">Loop End G-code</span>
    <textarea
      bind:value={settings.loop_end_gcode}
      rows="2"
      class="input-premium font-mono text-[10px] w-full resize-y min-h-[50px]"
    ></textarea>
  </div>
  <div class="flex flex-col gap-1">
    <span class="text-slate-400 font-bold uppercase text-[9px]">G-code po výměně trysky</span>
    <span class="text-[9px] text-slate-500 leading-snug">
      Vloží se po každé výměně trysky, hned za pauzou pro obsluhu. Homing je
      povinný — nová jehla má jinou délku i vyosení. Prázdné pole = G28.
    </span>
    <textarea
      bind:value={settings.nozzle_change_gcode}
      rows="2"
      placeholder="G28 ; Homing po vymene trysky"
      class="input-premium font-mono text-[10px] w-full resize-y min-h-[50px]"
    ></textarea>
  </div>
</div>
