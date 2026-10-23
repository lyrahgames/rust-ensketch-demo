import { resolve } from 'node:path';
import { defineConfig } from 'vite';

const appDirectory = import.meta.dirname;
const workspaceRoot = resolve(appDirectory, '../..');

export default defineConfig({
  root: workspaceRoot,
  server: {
    open: '/apps/web/',
    fs: { allow: [workspaceRoot] },
  },
  preview: {
    open: '/apps/web/',
  },
  build: {
    rollupOptions: {
      input: {
        webIndex: resolve(appDirectory, 'index.html'),
        case1: resolve(workspaceRoot, 'examples/web/case-1.html'),
        case2: resolve(workspaceRoot, 'examples/web/case-2.html'),
        case3: resolve(workspaceRoot, 'examples/web/case-3.html'),
        case4: resolve(workspaceRoot, 'examples/web/case-4.html'),
      },
    },
  },
});
