import { defineConfig } from '@rsbuild/core';
import { pluginReact } from '@rsbuild/plugin-react';

export default defineConfig({
  plugins: [pluginReact()],
  source: {
    entry: {
      index: './src/main.tsx',
    },
  },
  html: {
    title: '打印助手',
  },
  output: {
    distPath: {
      root: 'dist',
    },
  },
  server: {
    host: '127.0.0.1',
    port: 1420,
    // Keep the port stable so Tauri's devUrl does not miss the server.
    strictPort: true,
  },
});
