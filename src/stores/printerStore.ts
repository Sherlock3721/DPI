import { readable } from "svelte/store";
import { subscribe_printer_status, subscribe_print_error, type PrinterStatus } from "../lib/tauri";

const defaultStatus: PrinterStatus = {
  is_connected: false,
  is_printing: false,
  is_paused: false,
  current_x: 0.0,
  current_y: 0.0,
  current_z: 0.0,
  temp_extruder: 0.0,
  temp_bed: 0.0,
  progress: 0,
  total_dist: 0.0,
  time_remaining: 0.0,
};

/** Read-only stav tiskárny plněný z backend eventu `printer-status-changed`.
 *  Ovládání tiskárny (connect, tisk, pauza…) žije v LeftPanel — zde jen telemetrie. */
export const printerStore = readable<PrinterStatus>(defaultStatus, (set) => {
  const unlistenStatus = subscribe_printer_status(set).catch((err) => {
    console.error("Failed to setup printer status listener:", err);
    return null;
  });

  // Chyby komunikace (timeout, chyba zápisu, ztráta spojení) — zobrazíme uživateli
  const unlistenError = subscribe_print_error((message) => {
    console.error("Printer error:", message);
    alert(message);
  }).catch((err) => {
    console.error("Failed to setup print error listener:", err);
    return null;
  });

  return () => {
    unlistenStatus.then((fn) => fn?.());
    unlistenError.then((fn) => fn?.());
  };
});
