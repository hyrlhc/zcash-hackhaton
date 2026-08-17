import { defineConfig } from "vite";

export default defineConfig({
  root: ".",
  server: {
    port: 5173,
    strictPort: true,
    fs: { allow: ["../.."] },
    proxy: {
      "/api": "http://127.0.0.1:8787",
    },
  },
});
