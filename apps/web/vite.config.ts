import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

export default defineConfig(() => {
  return {
    plugins: [react()],
    clearScreen: false,
    resolve: {
      alias: {
        "@": path.resolve(__dirname, "src"),
        "@mcp_link/shared": path.resolve(
          __dirname,
          "../../packages/shared/src",
        ),
        "@mcp_link/tailwind-config": path.resolve(
          __dirname,
          "../../packages/tailwind-config",
        ),
        "@mcp_link/ui": path.resolve(__dirname, "../../packages/ui/src"),
      },
    },
    server: {
      host: "0.0.0.0",
      port: 1420,
      strictPort: true,
    },
    envPrefix: ["VITE_", "TAURI_"],
  };
});
