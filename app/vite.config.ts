import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["frontend/test/setup.ts"],
    include: ["frontend/**/*.test.{ts,tsx}"],
    // Per-test cap, setup hooks included (CLAUDE.md "Test policy"). Never raise it.
    testTimeout: 1000,
    hookTimeout: 1000,
  },
});
