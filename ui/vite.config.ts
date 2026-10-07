import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// In development, run `cargo run -p qrzero-server -- --token dev` and open
// http://localhost:5173/?token=dev. API calls are proxied to the server.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: { "/api": "http://127.0.0.1:8073" },
    fs: { allow: [".."] },
  },
  build: { outDir: "dist", emptyOutDir: true, chunkSizeWarningLimit: 1000 },
});
