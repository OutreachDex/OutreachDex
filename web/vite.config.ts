import { realpathSync } from 'node:fs'
import { dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwind from '@tailwindcss/vite'

// The editor packages are linked from a sibling checkout; let the dev server read them there.
const kitPackages = dirname(realpathSync(fileURLToPath(new URL('./node_modules/kit-host', import.meta.url))))

export default defineConfig({
  plugins: [react(), tailwind()],
  clearScreen: false,
  resolve: { dedupe: ['react', 'react-dom'] },
  optimizeDeps: { exclude: ['kit-host', 'kit-protocol'] },
  server: { fs: { allow: ['..', kitPackages] } },
})
