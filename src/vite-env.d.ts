/// <reference types="vite/client" />
/// <reference types="svelte" />

interface Window {
  /** Injektováno Tauri runtime — přítomnost rozlišuje desktop vs. web režim. */
  __TAURI_INTERNALS__?: unknown;
}
