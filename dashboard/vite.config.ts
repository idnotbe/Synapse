import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  base: "/dashboard/",
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src")
    }
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    cssCodeSplit: false,
    assetsInlineLimit: 1048576,
    rollupOptions: {
      output: {
        entryFileNames: "assets/dashboard-[hash].js",
        chunkFileNames: "assets/dashboard-[name]-[hash].js",
        assetFileNames: (assetInfo) => {
          if (assetInfo.names?.some((name) => name.endsWith(".css"))) {
            return "assets/dashboard-[hash].css";
          }
          return "assets/[name]-[hash][extname]";
        }
      }
    }
  }
});
