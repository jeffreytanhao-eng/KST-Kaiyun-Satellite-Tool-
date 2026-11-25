import { fileURLToPath, URL } from 'node:url'

import vue from '@vitejs/plugin-vue'
// 该插件 0.7.4 的 d.ts 使用 `export =`，与 ESM 默认导入不匹配（运行时导出正常）。
// @ts-expect-error 类型声明与 ESM 默认导出不一致
import cesium from 'vite-plugin-cesium-build'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [vue(), cesium({ iife: false })],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url))
    }
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:18080',
        changeOrigin: true
      }
    }
  }
})
