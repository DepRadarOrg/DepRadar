/// <reference types="vitest/config" />
import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv, type Plugin } from "vite";

/**
 * Fills `%SITE_URL%` in index.html with `VITE_SITE_URL` (with a trailing
 * slash), so social-preview tags get the absolute image URL crawlers need.
 * Unset, the tags fall back to a path relative to the page.
 */
function siteUrl(url: string | undefined): Plugin {
  const prefix = url ? url.replace(/\/*$/, "/") : "";
  return {
    name: "site-url",
    transformIndexHtml: (html) => html.replaceAll("%SITE_URL%", prefix),
  };
}

export default defineConfig(({ mode }) => ({
  plugins: [react(), siteUrl(loadEnv(mode, process.cwd()).VITE_SITE_URL)],
  // Relative asset paths so the build can be served from any sub-path.
  base: "./",
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
  },
}));
