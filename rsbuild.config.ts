import { defineConfig } from '@rsbuild/core';
import { pluginReact } from '@rsbuild/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [pluginReact()],
  source: {
    entry: {
      index: './src/main.tsx',
    },
  },
  html: {
    title: '打印助手',
    favicon: path.resolve(__dirname, 'src-tauri/icons/32x32.png'),
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
