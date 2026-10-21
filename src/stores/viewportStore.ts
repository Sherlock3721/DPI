import { writable } from "svelte/store";
import type { AdapterInfo, HostKind } from "../lib/viewport/host";

/** Aktivní renderer 3D náhledu (hostitel a GPU adaptér); null = nespuštěný. */
export const viewportRenderer = writable<(AdapterInfo & { kind: HostKind }) | null>(null);
