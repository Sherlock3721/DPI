<script lang="ts">
  import { run } from 'svelte/legacy';

  import { Target } from "lucide-svelte";

  interface Props {
    settings: any;
    glassList: { name: string; w: number; h: number; z: number }[];
    onGoToLeveling: () => void;
  }

  let { settings = $bindable(), glassList, onGoToLeveling }: Props = $props();

  let minBedMaxX = $state(0), minBedMaxY = $state(0);
  run(() => {
    const maxW = glassList.length > 0 ? Math.max(...glassList.map((g) => g.w)) : 0;
    const maxH = glassList.length > 0 ? Math.max(...glassList.map((g) => g.h)) : 0;
    minBedMaxX = Math.ceil((settings?.start_offset_x ?? 16) + Math.max(76, maxW));
    minBedMaxY = Math.ceil((settings?.start_offset_y ?? 12) + 26 + (settings?.multi_spacing ?? 5) + maxH);
  });
</script>

<div class="flex flex-col gap-5">
  <span class="font-bold text-xs text-slate-300 pb-1 border-b border-slate-800"
    >Limitace a parametry tiskárny</span
  >

  <!-- ROZMĚRY PODLOŽKY -->
  <div class="flex flex-col gap-3">
    <div class="flex items-center gap-2">
      <span class="w-2 h-2 rounded-full bg-blue-500"></span>
      <span class="text-xs font-bold text-slate-300 uppercase tracking-wider">Rozměry tiskové plochy</span>
    </div>
    <p class="text-[10px] text-slate-500 pl-4">
      Levá strana tisku je výchozí. Oblast roste doprava — sklíčka se přidávají ve sloupcích od levé strany.
    </p>
    <div class="grid grid-cols-1 gap-2.5 text-xs pl-4">
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Pravá hranice osy X</div>
          <div class="text-[10px] text-slate-500 mt-0.5">
            Pravý okraj tiskové plochy. Min: <span class="font-mono text-slate-400">{minBedMaxX} mm</span>
          </div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="5" min={minBedMaxX} bind:value={settings.bed_max_x}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-6">mm</span>
        </div>
      </div>
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Maximální délka osy Y</div>
          <div class="text-[10px] text-slate-500 mt-0.5">
            Fyzická délka podložky. Min: <span class="font-mono text-slate-400">{minBedMaxY} mm</span>
          </div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="5" min={minBedMaxY} bind:value={settings.bed_max_y}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-6">mm</span>
        </div>
      </div>
    </div>
  </div>

  <div class="border-b border-slate-800/60"></div>

  <!-- TEPLOTA -->
  <div class="flex flex-col gap-3">
    <div class="flex items-center gap-2">
      <span class="w-2 h-2 rounded-full bg-orange-500"></span>
      <span class="text-xs font-bold text-slate-300 uppercase tracking-wider">Teplota podložky</span>
    </div>
    <div class="grid grid-cols-1 gap-2.5 text-xs pl-4">
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Maximální teplota</div>
          <div class="text-[10px] text-slate-500 mt-0.5">Horní mez výhřevu. Hodnota 0 = bez limitu</div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="5" min="0" bind:value={settings.bed_max_temp}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-6">°C</span>
        </div>
      </div>
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Minimální teplota při zapnutí</div>
          <div class="text-[10px] text-slate-500 mt-0.5">
            Nejnižší nastavitelná teplota — krok pod ni výhřev vypne{settings.bed_min_temp > 0
              ? ` (přeskočí 0–${settings.bed_min_temp - 1} °C)`
              : "; 0 °C je platná teplota"}
          </div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="1" min="0" max="100" bind:value={settings.bed_min_temp}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-6">°C</span>
        </div>
      </div>
    </div>
  </div>

  <div class="border-b border-slate-800/60"></div>

  <!-- STARTOVNÍ POZICE -->
  <div class="flex flex-col gap-3">
    <div class="flex items-center gap-2">
      <span class="w-2 h-2 rounded-full bg-emerald-500"></span>
      <span class="text-xs font-bold text-slate-300 uppercase tracking-wider">Startovní pozice tisku</span>
    </div>
    <div class="grid grid-cols-1 gap-2.5 text-xs pl-4">
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Offset X</div>
          <div class="text-[10px] text-slate-500 mt-0.5">Posunutí výchozí pozice od nuly osy X</div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="1" bind:value={settings.start_offset_x}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-6">mm</span>
        </div>
      </div>
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Offset Y</div>
          <div class="text-[10px] text-slate-500 mt-0.5">Posunutí výchozí pozice od nuly osy Y</div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="1" bind:value={settings.start_offset_y}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-6">mm</span>
        </div>
      </div>
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Výška přesunu (cestovní Z)</div>
          <div class="text-[10px] text-slate-500 mt-0.5">Výška zdvihu trysky při přesunu mezi tisky</div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="0.5" bind:value={settings.block_height}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-6">mm</span>
        </div>
      </div>
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Mezera mezi substráty</div>
          <div class="text-[10px] text-slate-500 mt-0.5">Vzdálenost mezi substráty při multiplexním tisku</div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="0.5" bind:value={settings.multi_spacing}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-6">mm</span>
        </div>
      </div>
    </div>
    <button
      onclick={onGoToLeveling}
      class="flex items-center gap-1.5 text-[10px] text-labaccent hover:text-blue-300 transition-colors mt-0.5 w-fit"
    >
      <Target class="w-3 h-3" /> Upravit kalibrační body bed levelingu →
    </button>
  </div>

  <div class="border-b border-slate-800/60"></div>

  <!-- KALIBRACE -->
  <div class="flex flex-col gap-3">
    <div class="flex items-center gap-2">
      <span class="w-2 h-2 rounded-full bg-purple-500"></span>
      <span class="text-xs font-bold text-slate-300 uppercase tracking-wider">Kalibrace extruze</span>
    </div>
    <div class="grid grid-cols-1 gap-2.5 text-xs pl-4">
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Kalibrační faktor extruze</div>
          <div class="text-[10px] text-slate-500 mt-0.5">
            Konstanta přepočtu kroků motoru na objemový průtok (µl/krok)
          </div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="0.0001" bind:value={settings.calibration_factor}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-12">krok/µl</span>
        </div>
      </div>
    </div>
  </div>

  <div class="border-b border-slate-800/60"></div>

  <!-- PŘEJEZDY A DÁVKOVÁNÍ -->
  <div class="flex flex-col gap-3">
    <div class="flex items-center gap-2">
      <span class="w-2 h-2 rounded-full bg-cyan-500"></span>
      <span class="text-xs font-bold text-slate-300 uppercase tracking-wider">Přejezdy a dávkování kapaliny</span>
    </div>
    <p class="text-[10px] text-slate-500 pl-4">
      Uvnitř vyplňované plochy se tryska nezvedá (kapalina se táhne po místě, které výplň pokryje).
      Přes suché sklo se přejíždí se zdvihem a stažením kapaliny.
    </p>
    <div class="grid grid-cols-1 gap-2.5 text-xs pl-4">
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Zdvih trysky při přejezdu</div>
          <div class="text-[10px] text-slate-500 mt-0.5">
            Výška nad tiskovou výškou pro přejezdy přes suché sklo a zdvih po kapce
          </div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="0.1" min="0" bind:value={settings.default_z_hop}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-12">mm</span>
        </div>
      </div>
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Zpětné nasátí kapaliny</div>
          <div class="text-[10px] text-slate-500 mt-0.5">
            Objem stažený před přejezdem a vrácený po sjezdu — z trysky nevisí kapka (0 = vypnuto)
          </div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="1" min="0" bind:value={settings.retract_nl}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-12">nl</span>
        </div>
      </div>
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Rychlost nasátí</div>
          <div class="text-[10px] text-slate-500 mt-0.5">
            Posuv pístu při stažení a návratu kapaliny
          </div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="10" min="1" bind:value={settings.retract_speed}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-12">mm/min</span>
        </div>
      </div>
      <div class="grid grid-cols-5 items-center gap-3">
        <div class="col-span-3">
          <div class="text-slate-300 font-medium">Usazení kapky</div>
          <div class="text-[10px] text-slate-500 mt-0.5">
            Čekání po dávce kapky, než se tryska zvedne (styl Tečky)
          </div>
        </div>
        <div class="col-span-2 flex items-center gap-1.5">
          <input type="number" step="50" min="0" bind:value={settings.dot_dwell_ms}
            class="flex-1 input-premium py-1 text-center text-xs" />
          <span class="text-slate-500 text-[10px] w-12">ms</span>
        </div>
      </div>
    </div>
  </div>
</div>
