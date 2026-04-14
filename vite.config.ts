import { defineConfig } from "vite";

// https://vitejs.dev/config/
export default defineConfig({
  // Tauri expects a fixed port for the dev server
  server: {
    port: 1420,
    strictPort: true,
  },
  // Build output to dist/ for Tauri to pick up
  build: {
    outDir: "dist",
  },
  // Prevent vite from obscuring rust errors
  clearScreen: false,
  // Tauri uses a custom protocol, so we need to handle env variables differently
  envPrefix: ["VITE_", "TAURI_"],
});
