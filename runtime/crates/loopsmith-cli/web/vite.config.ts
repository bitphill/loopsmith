import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwind from "@tailwindcss/vite";

// The output lands inside the Rust crate's `src/` on purpose: `include_str!`
// cannot reach above the package root, and `config/` and `assets/` are excluded
// from the published tarball. Filenames are fixed rather than content-hashed
// for the same reason — an `include_str!` literal cannot chase a hash.
export default defineConfig({
  plugins: [react(), tailwind()],
  build: {
    outDir: "../src/web/dist",
    emptyOutDir: true,
    // One stylesheet, not one per chunk, so there are exactly three files to
    // compile in and no manifest to interpret at run time.
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        entryFileNames: "app.js",
        chunkFileNames: "app.js",
        assetFileNames: (info) =>
          info.names?.[0]?.endsWith(".css") ? "app.css" : "[name][extname]",
        // A single chunk keeps the three-file contract. This app is small
        // enough that splitting would buy nothing on localhost anyway.
        manualChunks: undefined,
      },
    },
  },
  server: {
    port: 5173,
    // `npm run dev` talks to a `loopsmith web --no-open` on 3000, so the
    // frontend can be iterated on without a Rust rebuild each time.
    proxy: {
      "/api": { target: "http://127.0.0.1:3000", ws: true },
    },
  },
});
