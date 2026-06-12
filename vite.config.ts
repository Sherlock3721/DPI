import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import wasm from "vite-plugin-wasm";

export default defineConfig({
  plugins: [wasm(), svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    // esnext: WebView2 i WebKitGTK podporují top-level await nativně —
    // nahrazuje dřívější vite-plugin-top-level-await (WASM ESM integrace).
    target: "esnext",
    rollupOptions: {
      output: {
        // Rolldown (Vite 8) podporuje jen funkční formu manualChunks
        manualChunks(id) {
          if (id.includes("node_modules/lucide-svelte")) return "vendor-icons";
          if (id.includes("node_modules/@tauri-apps")) return "vendor-tauri";
        },
      },
    },
  },
});
