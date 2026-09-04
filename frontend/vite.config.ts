import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// The gateway address is only used by the dev-server proxy; production
// builds are served behind nginx which proxies /api and /ws itself.
const gateway = process.env.EXCHANGEKIT_GATEWAY ?? "http://localhost:8080";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 3000,
    proxy: {
      "/api": { target: gateway, changeOrigin: true },
      "/ws": { target: gateway, changeOrigin: true, ws: true },
    },
  },
  test: {
    environment: "node",
  },
});
