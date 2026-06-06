import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
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
    rollupOptions: {
      output: {
        manualChunks: {
          "vendor-tauri": [
            "@tauri-apps/api",
            "@tauri-apps/plugin-dialog",
            "@tauri-apps/plugin-fs",
            "@tauri-apps/plugin-process",
            "@tauri-apps/plugin-shell",
            "@tauri-apps/plugin-updater",
          ],
          "vendor-icons": ["lucide-svelte"],
        },
      },
    },
  },
});
