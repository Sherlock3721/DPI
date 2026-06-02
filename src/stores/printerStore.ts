import { writable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import {
  connect_to_printer,
  auto_connect_printer,
  disconnect_from_printer,
  send_manual_command,
  start_print,
  pause_print,
  resume_print,
  stop_print,
  get_available_ports,
  type PrinterStatus,
} from "../lib/tauri";

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

function createPrinterStore() {
  const { subscribe, set, update } = writable<PrinterStatus>(defaultStatus);

  let unlistenStatus: (() => void) | null = null;

  // Initialize event listener for backend status updates
  listen("printer-status-changed", (event) => {
    set(event.payload as PrinterStatus);
  })
    .then((fn) => { unlistenStatus = fn; })
    .catch((err) => console.error("Failed to setup printer status listener:", err));

  return {
    subscribe,
    set,
    update,
    unlisten: () => { unlistenStatus?.(); unlistenStatus = null; },
    connect: async (portName: string, baudrate: number) => {
      try {
        const status = await connect_to_printer(portName, baudrate);
        set(status);
        return true;
      } catch (err) {
        console.error("Connect error:", err);
        alert(`Nepodařilo se připojit k tiskárně: ${err}`);
        return false;
      }
    },
    disconnect: async () => {
      try {
        await disconnect_from_printer();
      } catch (err) {
        console.error("Disconnect error:", err);
      }
    },
    sendGCode: async (gcode: string) => {
      try {
        await send_manual_command(gcode);
      } catch (err) {
        console.error("Send GCode error:", err);
      }
    },
    startJob: async (gcode: string, totalDist: number, totalTime: number) => {
      try {
        await start_print(gcode, totalDist, totalTime);
      } catch (err) {
        console.error("Start print error:", err);
        alert("Chyba při odesílání tisku.");
      }
    },
    pauseJob: async () => {
      try {
        await pause_print();
      } catch (err) {
        console.error(err);
      }
    },
    resumeJob: async () => {
      try {
        await resume_print();
      } catch (err) {
        console.error(err);
      }
    },
    stopJob: async () => {
      try {
        await stop_print();
      } catch (err) {
        console.error(err);
      }
    },
    getAvailablePorts: async () => {
      try {
        return await get_available_ports();
      } catch (err) {
        console.error("Failed to get ports:", err);
        return [];
      }
    },
  };
}

export const printerStore = createPrinterStore();
