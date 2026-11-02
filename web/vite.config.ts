import { resolve } from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      // The widget's unused optional Para loader imports this stylesheet.
      "@getpara/react-sdk/styles.css": resolve(
        import.meta.dirname,
        "src/optional-wallet.css",
      ),
    },
  },
  server: { port: 3004, strictPort: true },
});
