import { defineConfig, devices } from "@playwright/test";

/**
 * End-to-end tests drive the real binary, not a mock.
 *
 * `webServer` starts `loopsmith web` on a port of its own so a run of the
 * suite cannot collide with a browser someone left open on 3000, and
 * `--no-open` stops it launching a tab per test run.
 *
 * The binary must be built first: `cargo build -p loopsmith`. These tests are
 * deliberately about the seam between the browser and the CLI — anything that
 * can be checked in Rust is checked in Rust, where it is faster and does not
 * need a browser at all.
 */
const PORT = 3117;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  workers: 1,
  reporter: process.env.CI ? "line" : "list",
  timeout: 30_000,
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: `../../../target/debug/loopsmith web --no-open --port ${PORT}`,
    url: `http://127.0.0.1:${PORT}/api/meta`,
    reuseExistingServer: !process.env.CI,
    timeout: 20_000,
    // `import.meta.dirname`, not `__dirname`: package.json sets
    // "type": "module", so this config is ESM and has no CommonJS globals.
    cwd: import.meta.dirname,
  },
});
