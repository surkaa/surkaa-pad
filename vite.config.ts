import { defineConfig, loadEnv } from "vite";
import vue from "@vitejs/plugin-vue";
import {quasar, transformAssetUrls} from "@quasar/vite-plugin";
import {execFileSync} from "node:child_process";

function gitCommitHash() {
    const environmentHash = process.env.GITHUB_SHA || process.env.VITE_GIT_COMMIT;
    if (environmentHash) return environmentHash.slice(0, 8);
    try {
        return execFileSync('git', ['rev-parse', '--short=8', 'HEAD'], {
            cwd: process.cwd(),
            encoding: 'utf8',
        }).trim();
    } catch {
        return 'unknown';
    }
}

export default defineConfig(async ({mode}) => {
    const env = loadEnv(mode, process.cwd());
    const host = env.VITE_TAURI_DEV_HOST;
    const hmr = host ? {
        protocol: "ws",
        host,
        port: 5174,
    } : undefined;
    return {
        define: {
            __APP_GIT_COMMIT__: JSON.stringify(gitCommitHash()),
        },
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
                ignored: ["**/src-tauri/**", "**/.pnpm-store/**"],
            },
        },
    }
});
