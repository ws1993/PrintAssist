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
    port: 1420,
  },
});
