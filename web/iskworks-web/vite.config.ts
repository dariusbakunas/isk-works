import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    host: "127.0.0.1",
    port: 5173,
    proxy: {
      "/api": {
        target: process.env.VITE_API_BASE_URL ?? "http://127.0.0.1:8080",
        changeOrigin: true,
      },
    },
  },
  test: {
    api: false,
    environment: "jsdom",
    // Calendar tests assert "local time" behaviour, including DST transitions, so
    // pin a DST-observing zone instead of inheriting the machine's (CI runs in UTC).
    env: { TZ: "America/New_York" },
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    setupFiles: ["./src/test/setup.ts"],
  },
});
