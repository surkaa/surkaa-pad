import { defineConfig, loadEnv } from "vite";
import vue from "@vitejs/plugin-vue";
import {quasar, transformAssetUrls} from "@quasar/vite-plugin";

export default defineConfig(async ({mode}) => {
    const env = loadEnv(mode, process.cwd());
    const host = env.VITE_TAURI_DEV_HOST;
    const hmr = host ? {
        protocol: "ws",
        host,
        port: 5174,
    } : undefined;
    return {
        plugins: [
            vue({template: { transformAssetUrls }}),
            quasar({
                sassVariables: '/src/quasar-variables.sass'
            })
        ],
        clearScreen: false,
        server: {
            port: 5173,
            strictPort: true,
            host: host || false,
            hmr,
            watch: {
                ignored: ["**/src-tauri/**"],
            },
        },
    }
});
