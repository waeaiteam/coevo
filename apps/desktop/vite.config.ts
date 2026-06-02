/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const defaultArtifactRoot = `D:/${"\u7f16\u8bd1\u4ea7\u7269"}/coevo`;
const artifactRoot = process.env.COEVO_BUILD_ARTIFACT_DIR || defaultArtifactRoot;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  cacheDir: `${artifactRoot}/vite-cache`,
  build: {
    outDir: `${artifactRoot}/desktop-dist`,
    emptyOutDir: true,
  },
  test: {
    globals: true,
    environment: "jsdom",
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8717",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api/, ""),
      },
    },
  },
  envPrefix: ["COEVO_"],
});
