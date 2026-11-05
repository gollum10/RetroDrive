import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Tauri dev server settings. The Rust side (src-tauri, including target/) must not be
// watched by Vite: Cargo locks build outputs and the watcher crashes with EBUSY.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
});
