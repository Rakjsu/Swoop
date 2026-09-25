import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Porta fixa: o Tauri em dev aponta para ela (apps/swoop/tauri.conf.json → devUrl).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    // WebView2 (Windows) e WebKitGTK recentes; o painel do celular usa navegador moderno.
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: true,
  },
});
