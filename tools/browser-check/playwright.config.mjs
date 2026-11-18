import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: '.', testMatch: '*.spec.mjs', fullyParallel: false, workers: 1,
  timeout: 20000, expect: { timeout: 5000 },
  use: { channel: 'chrome', headless: true, viewport: { width: 1220, height: 820 }, baseURL: 'http://127.0.0.1:1421', screenshot: 'only-on-failure', trace: 'retain-on-failure' },
  outputDir: '../../artifacts/browser-results', reporter: [['list'], ['json', { outputFile: '../../artifacts/browser-results.json' }]],
});
