<script lang="ts" module>
  export type ParamIconName =
    | "substrate"
    | "samples"
    | "bedTemp"
    | "leveling"
    | "prime"
    | "zCalibration"
    | "nozzleHeight"
    | "extrusion"
    | "nozzleType"
    | "speed"
    | "infillStyle"
    | "infillDensity"
    | "infillAngle"
    | "port"
    | "baud"
    | "name"
    | "note"
    | "printOrder"
    | "snakeStart"
    | "nozzleLength"
    | "glassThickness"
    | "primeSize";
</script>

<script lang="ts">
  // Piktogramy parametrů v panelech nastavení (16 × 16, tah barvou textu).
  // Každý kreslí, co parametr ovlivňuje, aby šly parametry rozlišit i bez čtení.
  interface Props {
    name: ParamIconName;
    class?: string;
  }

  let { name, class: className = "w-3.5 h-3.5" }: Props = $props();
</script>

<svg
  viewBox="0 0 16 16"
  fill="none"
  stroke="currentColor"
  stroke-width="1.3"
  stroke-linecap="round"
  stroke-linejoin="round"
  class="shrink-0 {className}"
  aria-hidden="true"
>
  {#if name === "substrate"}
    <!-- Sklíčko v perspektivě s tloušťkou -->
    <path d="M1.5 10.5 5 6h9.5L11 10.5Z" />
    <path d="M1.5 10.5V12H11l3.5-4.5V6M11 10.5V12" />
  {:else if name === "samples"}
    <!-- Tři sklíčka vedle sebe -->
    <rect x="1.5" y="3" width="3.5" height="10" rx=".5" />
    <rect x="6.25" y="3" width="3.5" height="10" rx=".5" />
    <rect x="11" y="3" width="3.5" height="10" rx=".5" />
  {:else if name === "bedTemp"}
    <!-- Podložka a stoupající teplo -->
    <path d="M1.5 13.5h13" />
    <path d="M4.5 11c-1-1.2 1-2.3 0-3.5s1-2.3 0-3.5M8 11c-1-1.2 1-2.3 0-3.5s1-2.3 0-3.5M11.5 11c-1-1.2 1-2.3 0-3.5s1-2.3 0-3.5" />
  {:else if name === "leveling"}
    <!-- Podložka s měřicí sítí -->
    <path d="M1.5 12 4.5 5h10l-3 7Z" />
    <path d="M8.3 5 6.5 12M11.5 5 9.6 12M3 8.5h10" />
  {:else if name === "prime"}
    <!-- Tryska odkapává na odplivové sklo -->
    <path d="M5.5 1.5h5V4L9 6.5H7L5.5 4Z" />
    <path d="M8 8.3c.9 1 1.3 1.7 1.3 2.3a1.3 1.3 0 0 1-2.6 0c0-.6.4-1.3 1.3-2.3Z" />
    <path d="M2.5 14.5h11" />
  {:else if name === "zCalibration"}
    <!-- Tryska na povrchu, posun nahoru/dolů -->
    <path d="M6.5 1.5h6V5l-2 3.5h-2l-2-3.5Z" />
    <path d="M1.5 10.5h13" />
    <path d="M3 2.5v5M1.8 3.7 3 2.5l1.2 1.2M1.8 6.3 3 7.5l1.2-1.2" />
    <path d="M4 13.5h8" stroke-dasharray="1.5 1.5" />
  {:else if name === "nozzleHeight"}
    <!-- Mezera mezi hrotem trysky a sklem -->
    <path d="M3 1.5h6v3.5L7 8.5H5L3 5Z" />
    <path d="M1.5 14h13" />
    <path d="M12.5 8.5v5.5M11.2 8.5h2.6M11.3 9.8l1.2-1.3 1.2 1.3M11.3 12.7l1.2 1.3 1.2-1.3" />
  {:else if name === "extrusion"}
    <!-- Injekční stříkačka s pístem -->
    <path d="M5.5 1.5h5M8 1.5v3" />
    <rect x="5.5" y="4.5" width="5" height="6.5" rx=".5" />
    <path d="M8 11v3.5M5.5 7h1.8M5.5 9h1.8" />
  {:else if name === "nozzleType"}
    <!-- Tryska a průměr otvoru -->
    <path d="M4 1.5h8v3L9.5 8h-3L4 4.5Z" />
    <circle cx="8" cy="12" r="2.3" />
    <path d="M6 14 10 10" />
  {:else if name === "speed"}
    <!-- Tryska v pohybu -->
    <path d="M8 2h5.5v3L12 8h-2.5L8 5Z" />
    <path d="M1.5 3.5h4.5M2.5 6h3.5M1.5 8.5h4.5" />
    <path d="M1.5 12h13" />
  {:else if name === "infillStyle"}
    <!-- Obrys s hadovitou výplní -->
    <rect x="1.5" y="1.5" width="13" height="13" rx="1" />
    <path d="M4.5 4.5h7v3.5h-7v3.5h7" />
  {:else if name === "infillDensity"}
    <!-- Rozestup řad výplně -->
    <path d="M1.5 3h8.5M1.5 6.5h8.5M1.5 10h8.5M1.5 13.5h8.5" />
    <path d="M13 3v3.5M11.8 4.2 13 3l1.2 1.2M11.8 5.3 13 6.5l1.2-1.2" />
  {:else if name === "infillAngle"}
    <!-- Natočené řady a oblouk úhlu -->
    <path d="M1.5 14.5h13" />
    <path d="M3 14.5 10 4.5M8.5 14.5l5-7.2" />
    <path d="M7.5 14.5a4.5 4.5 0 0 0-1.9-3.7" />
  {:else if name === "port"}
    <!-- USB konektor -->
    <path d="M6 1.5h4v4H6Z" />
    <path d="M4.5 5.5h7v4l-2 2.5h-3l-2-2.5Z" />
    <path d="M8 12v2.5" />
  {:else if name === "baud"}
    <!-- Datový signál -->
    <path d="M1.5 11h2V5h3v6h3V5h3v6h2" />
  {:else if name === "name"}
    <!-- Štítek se jménem -->
    <path d="M1.5 4.5h8.5l4.5 3.5-4.5 3.5H1.5Z" />
    <path d="M4 8h3.5" />
  {:else if name === "note"}
    <!-- Lístek s textem -->
    <path d="M2.5 1.5h8l3 3v10h-11Z" />
    <path d="M5 6.5h6M5 9h6M5 11.5h3.5" />
  {:else if name === "printOrder"}
    <!-- Očíslované pořadí -->
    <path d="M2.5 2.5h1v3.5M2.2 6h1.8M2 9.3c.3-.6 1.9-.7 1.9.3 0 .8-1.9 1.4-1.9 2.4h2" />
    <path d="M7 4h7.5M7 10.5h7.5" />
  {:else if name === "snakeStart"}
    <!-- Začátek hada v rohu -->
    <circle cx="3" cy="13" r="1.5" fill="currentColor" stroke="none" />
    <path d="M4.5 13h9.5v-3.5H2v-3.5h12V2.5H2" />
  {:else if name === "nozzleLength"}
    <!-- Délka jehly od držáku po hrot -->
    <path d="M3 1.5h6v2.5H3Z" />
    <path d="M5.3 4v8.5L6 14.5l.7-2V4" />
    <path d="M12 1.5v13M10.8 2.7 12 1.5l1.2 1.2M10.8 13.3l1.2 1.2 1.2-1.2" />
  {:else if name === "glassThickness"}
    <!-- Průřez sklem s kótou tloušťky -->
    <path d="M1.5 6.5h9.5v4H1.5Z" />
    <path d="M1.5 13.5h13" />
    <path d="M13 3v3.5M13 10.5V13M11.8 5.3 13 6.5l1.2-1.2M11.8 11.7l1.2-1.2 1.2 1.2" />
  {:else if name === "primeSize"}
    <!-- Odplivový čtverec s kótou strany -->
    <rect x="2.5" y="5" width="9" height="9" rx=".5" />
    <path d="M2.5 2h9M3.7 .8 2.5 2l1.2 1.2M10.3.8 11.5 2l-1.2 1.2" />
    <path d="M5 11.5c1-2.5 3-2.5 4 0" />
  {/if}
</svg>
