<script lang="ts">
  import { ShieldAlert, Moon, Sun } from "lucide-svelte";
  import { viewportRenderer } from "../../stores/viewportStore";

  interface Props {
    settings: any;
    expertModeActive: boolean;
    snowSeason: boolean;
    snowDisabled: boolean;
    currentTheme: "dark" | "light";
    onRequestExpertMode: () => void;
    onDisableExpertMode: () => void;
    onApplyTheme: (t: "dark" | "light") => void;
  }

  let {
    settings = $bindable(),
    expertModeActive,
    snowSeason,
    snowDisabled = $bindable(),
    currentTheme,
    onRequestExpertMode,
    onDisableExpertMode,
    onApplyTheme
  }: Props = $props();

  const RENDER_HOSTS = [
    { value: "auto", label: "Automaticky", hint: "Linux: v aplikaci, jinak ve WebView" },
    { value: "native", label: "V aplikaci (wgpu)", hint: "Nezávislé na WebGL ve WebView, snímky se přenáší" },
    { value: "wasm", label: "Ve WebView (WebGL2)", hint: "Bez přenosu snímků, vyžaduje funkční WebGL2" },
  ];
  const VIEW_TOGGLES = [
    { key: "auto_perspective", label: "Automatická perspektiva", hint: "Otočení z ortho pohledu přepne na perspektivu, pohled po ose (Numpad 1/3/7) zpět na ortho" },
    { key: "center_on_click", label: "Vycentrovat po kliknutí", hint: "Klik na substrát nebo objekt plynule posune pohled a nastaví na něj střed otáčení" },
  ];
</script>

<div class="flex flex-col gap-6">
  <span class="font-bold text-xs text-slate-300 pb-1 border-b border-slate-800"
    >Nastavení aplikace</span
  >

  <!-- EXPERTNÍ REŽIM -->
  <div class="flex flex-col gap-3">
    <span class="text-xs font-bold text-slate-400 uppercase tracking-wider">Expertní režim</span>
    <div class="rounded-xl border-2 p-4 flex flex-col gap-3 transition-all
                 {expertModeActive ? 'border-labred/50 bg-labred/5' : 'border-slate-700 bg-slate-900/40'}">
      <div class="flex items-center justify-between gap-4">
        <div class="flex items-center gap-2.5">
          <ShieldAlert class="w-4 h-4 shrink-0 {expertModeActive ? 'text-labred' : 'text-slate-500'}" />
          <div>
            <p class="text-xs font-bold {expertModeActive ? 'text-labred' : 'text-slate-300'}">Aktivovat expertní režim</p>
            <p class="text-[10px] text-slate-500 mt-0.5">Zpřístupní úpravu inicializačních G-kódů tiskárny. Platí jen pro toto spuštění.</p>
          </div>
        </div>
        <button
          onclick={expertModeActive ? onDisableExpertMode : onRequestExpertMode}
          aria-label="Aktivovat expertní režim"
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 transition-colors focus:outline-hidden
                 {expertModeActive ? 'border-labred bg-labred/80' : 'border-slate-600 bg-slate-700'}"
        >
          <span class="inline-block h-3.5 w-3.5 transform rounded-full bg-white shadow transition-transform mt-px
                       {expertModeActive ? 'translate-x-4' : 'translate-x-0.5'}"></span>
        </button>
      </div>
      {#if expertModeActive}
        <div class="flex items-start gap-2 rounded-lg bg-labred/10 border border-labred/30 px-3 py-2">
          <ShieldAlert class="w-3.5 h-3.5 text-labred shrink-0 mt-0.5" />
          <p class="text-[10px] text-labred/90 leading-relaxed">
            Expertní režim je aktivní. Nesprávná úprava G-kódů může poškodit tiskárnu nebo způsobit nebezpečné pohyby. Režim se deaktivuje po zavření aplikace.
          </p>
        </div>
      {/if}
    </div>
  </div>

  <div class="border-b border-slate-800/60"></div>

  <!-- THEME -->
  <div class="flex flex-col gap-3">
    <span class="text-xs font-bold text-slate-400 uppercase tracking-wider">Barevný motiv</span>
    <div class="grid grid-cols-2 gap-3">
      <!-- DARK -->
      <button
        onclick={() => onApplyTheme("dark")}
        class="relative flex flex-col gap-3 p-4 rounded-xl border-2 transition-all cursor-pointer
               {currentTheme === 'dark'
          ? 'border-labaccent bg-labaccent/10 shadow-lg shadow-labaccent/10'
          : 'border-slate-700 bg-slate-900/40 hover:border-slate-600'}"
      >
        {#if currentTheme === "dark"}
          <span class="absolute top-2 right-2 w-2 h-2 rounded-full bg-labaccent"></span>
        {/if}
        <div class="w-full h-16 rounded-lg overflow-hidden border border-slate-700 flex flex-col">
          <div class="h-4 bg-slate-950 flex items-center gap-1 px-2">
            <span class="w-1.5 h-1.5 rounded-full bg-red-500"></span>
            <span class="w-1.5 h-1.5 rounded-full bg-yellow-500"></span>
            <span class="w-1.5 h-1.5 rounded-full bg-green-500"></span>
          </div>
          <div class="flex-1 bg-slate-900 flex gap-1 p-1">
            <div class="w-8 bg-slate-800 rounded-sm"></div>
            <div class="flex-1 bg-slate-950/50 rounded-sm"></div>
            <div class="w-8 bg-slate-800 rounded-sm"></div>
          </div>
        </div>
        <div class="flex items-center gap-2">
          <Moon class="w-4 h-4 text-slate-300" />
          <div>
            <p class="text-xs font-bold text-slate-200">Tmavý</p>
            <p class="text-[10px] text-slate-500">Výchozí laboratorní motiv</p>
          </div>
        </div>
      </button>

      <!-- LIGHT -->
      <button
        onclick={() => onApplyTheme("light")}
        class="relative flex flex-col gap-3 p-4 rounded-xl border-2 transition-all cursor-pointer
               {currentTheme === 'light'
          ? 'border-labaccent bg-labaccent/10 shadow-lg shadow-labaccent/10'
          : 'border-slate-700 bg-slate-900/40 hover:border-slate-600'}"
      >
        {#if currentTheme === "light"}
          <span class="absolute top-2 right-2 w-2 h-2 rounded-full bg-labaccent"></span>
        {/if}
        <div class="w-full h-16 rounded-lg overflow-hidden border border-slate-300 flex flex-col">
          <div class="h-4 bg-gray-100 flex items-center gap-1 px-2">
            <span class="w-1.5 h-1.5 rounded-full bg-red-500"></span>
            <span class="w-1.5 h-1.5 rounded-full bg-yellow-500"></span>
            <span class="w-1.5 h-1.5 rounded-full bg-green-500"></span>
          </div>
          <div class="flex-1 bg-white flex gap-1 p-1">
            <div class="w-8 bg-gray-100 rounded-sm"></div>
            <div class="flex-1 bg-gray-50 rounded-sm"></div>
            <div class="w-8 bg-gray-100 rounded-sm"></div>
          </div>
        </div>
        <div class="flex items-center gap-2">
          <Sun class="w-4 h-4 text-yellow-400" />
          <div>
            <p class="text-xs font-bold text-slate-200">Světlý</p>
            <p class="text-[10px] text-slate-500">Čistý bílý motiv</p>
          </div>
        </div>
      </button>
    </div>
    <p class="text-[10px] text-slate-500">
      Motiv se projeví okamžitě a zapamatuje si pro příští spuštění.
    </p>
  </div>


  <div class="border-b border-slate-800/60"></div>

  <!-- 3D NÁHLED -->
  <div class="flex flex-col gap-3">
    <span class="text-xs font-bold text-slate-400 uppercase tracking-wider">3D náhled</span>
    <div class="rounded-xl border border-slate-700 bg-slate-900/40 p-4 flex flex-col gap-4">
      <div class="flex flex-col gap-2">
        <p class="text-xs font-bold text-slate-300">Vykreslování</p>
        <div class="grid grid-cols-3 gap-2">
          {#each RENDER_HOSTS as h (h.value)}
            <button
              type="button"
              title={h.hint}
              onclick={() => (settings.render_host = h.value)}
              class="rounded-lg border px-2 py-1.5 text-[11px] font-bold transition-colors
                     {(settings.render_host ?? 'auto') === h.value
                ? 'border-labaccent bg-labaccent/15 text-slate-100'
                : 'border-slate-700 text-slate-400 hover:border-slate-600'}"
            >
              {h.label}
            </button>
          {/each}
        </div>
        <p class="text-[10px] text-slate-500">
          {RENDER_HOSTS.find((h) => h.value === (settings.render_host ?? "auto"))?.hint}
        </p>
        <p class="text-[10px] text-slate-400">
          {#if $viewportRenderer}
            Běží: {$viewportRenderer.kind === "native" ? "v aplikaci" : "ve WebView"} · {$viewportRenderer.name}
            ({$viewportRenderer.backend}, {$viewportRenderer.device_type})
          {:else}
            Renderer neběží.
          {/if}
        </p>
      </div>

      <label class="flex items-center justify-between gap-4">
        <div>
          <p class="text-xs font-bold text-slate-300">Převýšení výšky tras</p>
          <p class="text-[10px] text-slate-500 mt-0.5">Housenky jsou tenké (setiny mm) — převýšení je zviditelní, sklo zůstává ve skutečné tloušťce.</p>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <input type="range" min="1" max="20" step="1" bind:value={settings.z_exaggeration} class="w-28 accent-blue-500" />
          <span class="text-xs font-mono text-slate-300 w-8 text-right">{settings.z_exaggeration ?? 1}×</span>
        </div>
      </label>

      {#each VIEW_TOGGLES as t (t.key)}
        {@const on = settings[t.key] ?? true}
        <div class="flex items-center justify-between gap-4">
          <div>
            <p class="text-xs font-bold text-slate-300">{t.label}</p>
            <p class="text-[10px] text-slate-500 mt-0.5">{t.hint}</p>
          </div>
          <button
            type="button"
            onclick={() => (settings[t.key] = !on)}
            aria-label={t.label}
            class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 transition-colors focus:outline-hidden
                   {on ? 'border-labaccent bg-labaccent/80' : 'border-slate-600 bg-slate-700'}"
          >
            <span class="inline-block h-3.5 w-3.5 transform rounded-full bg-white shadow transition-transform mt-px
                         {on ? 'translate-x-4' : 'translate-x-0.5'}"></span>
          </button>
        </div>
      {/each}
    </div>
  </div>

  {#if snowSeason}
    <div class="border-b border-slate-800/60"></div>

    <!-- SNĚŽENÍ -->
    <div class="flex flex-col gap-3">
      <span class="text-xs font-bold text-slate-400 uppercase tracking-wider">Sezónní efekty</span>
      <div class="flex items-center justify-between gap-4 rounded-xl border border-slate-700 bg-slate-900/40 px-4 py-3">
        <div class="flex items-center gap-2.5">
          <span class="text-lg leading-none select-none">❄️</span>
          <div>
            <p class="text-xs font-bold text-slate-300">Vypnout sněžení</p>
            <p class="text-[10px] text-slate-500 mt-0.5">Efekt padajícího sněhu (aktivní 15. 11. – 30. 1.)</p>
          </div>
        </div>
        <button
          type="button"
          onclick={() => (snowDisabled = !snowDisabled)}
          aria-label="Vypnout sněžení"
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 transition-colors focus:outline-hidden
                 {snowDisabled ? 'border-slate-600 bg-slate-700' : 'border-labaccent bg-labaccent/80'}"
        >
          <span class="inline-block h-3.5 w-3.5 transform rounded-full bg-white shadow transition-transform mt-px
                       {snowDisabled ? 'translate-x-0.5' : 'translate-x-4'}"></span>
        </button>
      </div>
    </div>
  {/if}
</div>
