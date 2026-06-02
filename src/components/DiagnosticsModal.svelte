<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { X, AlertTriangle, Activity, Info, Move, Fan, Thermometer } from "lucide-svelte";
  import { send_manual_command } from "../lib/tauri";
  import Terminal from "./Terminal.svelte";
  import { printerStore } from "../stores/printerStore";

  export let isOpen = false;

  const dispatch = createEventDispatcher();

  function close() {
    isOpen = false;
    dispatch("close");
  }

  async function sendCommand(gcode: string) {
    try {
      const commands = gcode.split('\n');
      for (const cmd of commands) {
        if (cmd.trim() !== '') {
          await send_manual_command(cmd.trim());
        }
      }
    } catch (e) {
      console.error("Nepodařilo se odeslat příkaz:", e);
      alert(`Nepodařilo se odeslat příkaz: ${e}`);
    }
  }

  // Seznam diagnostických a manipulačních příkazů
  const commandGroups = [
    {
      title: "Informace a stav",
      icon: Info,
      color: "text-blue-400",
      commands: [
        {
          name: "Informace o tiskárně a FW",
          code: "M115",
          desc: "Hodí se k ověření spojení a správné verze firmwaru při řešení potíží.",
        },
        {
          name: "Stav koncových spínačů",
          code: "M119",
          desc: "Hodí se, pokud se tiskárna odmítá hýbat nebo naráží do krajů (kontrola senzorů).",
        },
        {
          name: "Aktuální pozice hlavy",
          code: "M114",
          desc: "Hodí se pro přesné zjištění aktuálních fyzických souřadnic X, Y a Z.",
        },
      ],
    },
    {
      title: "Pohyb a dávkování (kapaliny)",
      icon: Move,
      color: "text-green-400",
      commands: [
        {
          name: "Najetí domů (Všechny osy)",
          code: "G28",
          desc: "Základní reset polohy. Hodí se po zapnutí nebo ztrátě pozice.",
        },
        {
          name: "Najetí domů (Bez kalibrace podložky)",
          code: "G28 W",
          desc: "Rychlejší reset polohy domů. Hodí se, pokud nechcete čekat na měření podložky.",
        },
        {
          name: "Vynulovat dávkovač (Extruder)",
          code: "G92 E0",
          desc: "Hodí se po výměně nebo plnění stříkačky k vynulování počítadla vytlačené kapaliny.",
        },
        {
          name: "Uvolnit motory",
          code: "M84",
          desc: "Hodí se, když potřebujete manuálně a jemně posunout tiskovou hlavou nebo stříkačkou.",
        },
      ],
    },
    {
      title: "Teplota a chlazení",
      icon: Thermometer,
      color: "text-purple-400",
      commands: [
        {
          name: "Vypnout výhřev podložky",
          code: "M140 S0",
          desc: "Hodí se pro okamžité a bezpečné vypnutí vyhřívání sklíčka.",
        },
        {
          name: "Zapnout větráček (100%)",
          code: "M106 S255",
          desc: "Hodí se pro rychlé zchlazení vzorku na sklíčku nebo urychlení odpařování.",
        },
        {
          name: "Vypnout větráček",
          code: "M107",
          desc: "Hodí se k zamezení nežádoucího průvanu u citlivých chemických látek.",
        },
      ],
    },
    {
      title: "Nebezpečné operace (Obezřetně!)",
      icon: AlertTriangle,
      color: "text-red-500",
      commands: [
        {
          name: "Vypnout ochranné limity os",
          code: "M211 S0",
          desc: "Umožní vyjet mimo maximální rozměry. Hrozí drtivý náraz do vybavení!",
          danger: true,
        },
        {
          name: "Zapnout ochranné limity os",
          code: "M211 S1",
          desc: "Obnoví softwarové ochranné limity.",
        },
        {
          name: "Tovární nastavení EEPROM",
          code: "M502",
          desc: "Smaže interní kalibrace (např. kroky stříkačky) a vrátí výchozí hodnoty.",
          danger: true,
        },
        {
          name: "Uložit aktuální nastavení",
          code: "M500",
          desc: "Trvale uloží nové parametry (např. po kalibraci dávkovače), aby nezmizely po restartu.",
        },
        {
          name: "Obejití prvního hlášky o první kalibraci",
          code: "D3 Ax0ca6 X1f\nD3 Ax0ff7 X01\nD3 Ax0f5f X00\nD3 Ax0fa6 X01\nM500",
          codeLabel: "D3...M500",
          desc: "Nastaví status kalibrace na hotovo, přepíše EEPROM a vypne průvodce.",
          danger: true,
        },
      ],
    },
  ];

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) close();
  }
</script>

{#if isOpen}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div
    class="fixed inset-0 bg-slate-950/80 backdrop-blur-md z-[100] flex items-center justify-center p-4 sm:p-6"
    on:click={handleBackdropClick}
  >
    <div
      class="bg-slate-900 border border-slate-700/50 shadow-2xl rounded-2xl w-full max-w-4xl max-h-[90vh] flex flex-col overflow-hidden animate-in fade-in zoom-in-95 duration-200"
    >
      <!-- HLAVIČKA -->
      <div
        class="flex items-center justify-between px-6 py-4 border-b border-slate-800 bg-slate-900/50"
      >
        <div class="flex items-center gap-3">
          <div
            class="w-10 h-10 rounded-xl bg-blue-500/20 flex items-center justify-center text-blue-400"
          >
            <Activity class="w-5 h-5" />
          </div>
          <div>
            <h2 class="text-lg font-bold text-slate-100">Diagnostika tiskárny</h2>
            <p class="text-[11px] text-slate-400">
              Manuální odesílání diagnostických příkazů (výstup je zobrazen v Terminálu)
            </p>
          </div>
        </div>
        <button
          on:click={close}
          class="p-2 text-slate-400 hover:text-white hover:bg-slate-800 rounded-lg transition-colors outline-none"
        >
          <X class="w-5 h-5" />
        </button>
      </div>

      <!-- TĚLO S ROZDĚLENÝM LAYOUTEM -->
      <div class="flex-1 flex flex-col overflow-hidden bg-slate-900/40">
        {#if !$printerStore.is_connected}
          <div class="flex-1 flex flex-col items-center justify-center p-10 text-slate-400 gap-4">
            <AlertTriangle class="w-12 h-12 text-slate-500 opacity-50" />
            <p class="text-lg">Tiskárna není připojena</p>
            <p class="text-sm">
              Pro využití diagnostických příkazů a sériové konzole se prosím nejprve připojte k
              tiskárně.
            </p>
          </div>
        {:else}
          <!-- SÉRIOVÁ KONZOLE (Nahoře na šířku) -->
          <div class="w-full shrink-0 h-[240px] border-b border-slate-800/50 p-6 bg-slate-950/30">
            <Terminal />
          </div>

          <!-- PŘÍKAZY (Spodní část s posuvníkem) -->
          <div class="flex-1 overflow-y-auto p-6 custom-scrollbar">
            <div class="flex flex-col gap-8">
              {#each commandGroups as group}
                <div class="flex flex-col gap-3">
                  <div class="flex items-center gap-2 border-b border-slate-800/50 pb-2">
                    <svelte:component this={group.icon} class="w-4 h-4 {group.color}" />
                    <h3 class="font-bold text-sm text-slate-300 uppercase tracking-wider">
                      {group.title}
                    </h3>
                  </div>

                  <div class="grid grid-cols-1 xl:grid-cols-2 gap-3">
                    {#each group.commands as cmd}
                      <div
                        class="flex items-center justify-between p-3 rounded-xl border transition-all {cmd.danger
                          ? 'bg-red-950/30 border-red-900/50 hover:bg-red-900/40'
                          : 'bg-slate-800/30 border-slate-700/50 hover:bg-slate-800/60'}"
                      >
                        <div class="flex flex-col gap-1 pr-4">
                          <div class="flex items-center gap-2">
                            <span
                              class="font-bold text-sm {cmd.danger
                                ? 'text-red-400'
                                : 'text-slate-200'}">{cmd.name}</span
                            >
                            {#if cmd.danger}
                              <AlertTriangle class="w-3.5 h-3.5 text-red-500" />
                            {/if}
                          </div>
                          <span
                            class="text-[10px] {cmd.danger ? 'text-red-300/70' : 'text-slate-400'}"
                            >{cmd.desc}</span
                          >
                        </div>

                        <button
                          on:click={() => sendCommand(cmd.code)}
                          class="shrink-0 flex items-center gap-2 px-3 py-1.5 rounded-lg border font-mono text-xs font-bold transition-colors shadow-lg {cmd.danger
                            ? 'bg-red-600 border-red-500 text-white hover:bg-red-500 shadow-red-900/20'
                            : 'bg-slate-950 border-slate-700 text-labaccent hover:border-labaccent hover:bg-slate-900 shadow-black/20'}"
                        >
                          {cmd.codeLabel || cmd.code}
                        </button>
                      </div>
                    {/each}
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .custom-scrollbar::-webkit-scrollbar {
    width: 6px;
  }
  .custom-scrollbar::-webkit-scrollbar-track {
    background: transparent;
  }
  .custom-scrollbar::-webkit-scrollbar-thumb {
    background: rgba(51, 65, 85, 0.8);
    border-radius: 10px;
  }
</style>
