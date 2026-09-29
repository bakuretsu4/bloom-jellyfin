import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed dev port and must not see src-tauri changes as frontend changes.
const host = process.env.TAURI_DEV_HOST;

// Dev only: artwork for the mock backend (src/dev/mock.ts), drawn from the item id so the same
// title always gets the same picture.
function mockImages(): Plugin {
  return {
    name: "bloom-mock-images",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use("/mock-img", (req, res) => {
        const [id = "x", kind = "Primary"] = (req.url ?? "").split("/").filter(Boolean);
        let h = 0;
        for (const c of id) h = (h * 31 + c.charCodeAt(0)) >>> 0;
        const hue = h % 360;
        const [w, hgt] = kind === "Primary" ? [400, 600] : [960, 540];
        res.setHeader("Content-Type", "image/svg+xml");
        res.end(`<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${hgt}" viewBox="0 0 ${w} ${hgt}"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${hue} 55% 42%)"/><stop offset="1" stop-color="hsl(${(hue + 50) % 360} 60% 18%)"/></linearGradient></defs><rect width="100%" height="100%" fill="url(#g)"/><circle cx="${w * 0.7}" cy="${hgt * 0.35}" r="${hgt * 0.22}" fill="hsl(${(hue + 30) % 360} 70% 70% / .35)"/></svg>`);
      });
    },
  };
}

export default defineConfig({
  plugins: [svelte(), mockImages()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
