import { resolve } from "node:path";
import { defineConfig, loadEnv, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

/** In dev, /app serves app/index.html the way Cloudflare's auto-trailing-slash does. */
const appRoute: Plugin = {
  name: "dottie-app-route",
  configureServer(server) {
    server.middlewares.use((req, _res, next) => {
      if (req.url === "/app" || req.url?.startsWith("/app?") || req.url === "/app/" || req.url?.startsWith("/app/?"))
        req.url = "/app/index.html" + (req.url.includes("?") ? req.url.slice(req.url.indexOf("?")) : "");
      next();
    });
  },
};

export default defineConfig(({ mode }) => {
  // Share cards need an absolute image URL; SITE_URL overrides it for previews.
  const siteUrl = (loadEnv(mode, process.cwd(), "").SITE_URL || "https://soldottie.com").replace(/\/$/, "");
  return {
    plugins: [
      react(),
      appRoute,
      { name: "site-url", transformIndexHtml: (html: string) => html.replaceAll("%SITE_URL%", siteUrl) },
    ],
    resolve: {
      alias: {
        // The widget's unused optional Para loader imports this stylesheet.
        "@getpara/react-sdk/styles.css": resolve(import.meta.dirname, "src/optional-wallet.css"),
      },
    },
    build: {
      rollupOptions: {
        input: {
          landing: resolve(import.meta.dirname, "index.html"),
          app: resolve(import.meta.dirname, "app/index.html"),
        },
      },
    },
    server: { port: 3004, strictPort: true },
  };
});
