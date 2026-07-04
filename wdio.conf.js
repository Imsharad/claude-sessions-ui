// E2E harness for the Tauri app — embedded WebDriver on macOS (tauri-plugin-wdio-webdriver).
// Runs the compiled debug binary directly; no external driver. Plain ESM, no tsx.
import { spawn } from 'node:child_process';
import process from 'node:process';

let vite;

async function waitForVite(url, timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const r = await fetch(url);
      if (r.ok) return;
    } catch { /* not up yet */ }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`vite dev server not ready at ${url} within ${timeoutMs}ms`);
}

export const config = {
  runner: 'local',
  specs: ['./e2e/**/*.e2e.js'],
  maxInstances: 1, // ponytail: single instance — the app is a singleton desktop window
  capabilities: [
    {
      browserName: 'tauri',
      'tauri:options': {
        // Debug binary loads the frontend from devUrl (localhost:1420), so vite must
        // be running (started in onPrepare below).
        application: './src-tauri/target/debug/claude-sessions-ui',
      },
    },
  ],
  services: [['@wdio/tauri-service', { driverProvider: 'embedded' }]],
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: { ui: 'bdd', timeout: 120000 },
  logLevel: 'warn',
  waitforTimeout: 15000,

  // ponytail: the debug Tauri binary loads from devUrl, not the embedded dist, so we
  // serve the frontend with vite for the duration of the run. Ceiling: to test the
  // SHIPPED release bundle instead, move the WebDriver plugin behind a cargo feature,
  // build `--release --features e2e`, and drop these two hooks.
  onPrepare: async () => {
    vite = spawn('npm', ['run', 'dev'], { stdio: 'ignore', detached: true });
    await waitForVite('http://localhost:1420', 30000);
  },
  onComplete: () => {
    if (vite?.pid) {
      try { process.kill(-vite.pid); } catch { /* already gone */ }
    }
  },
};
