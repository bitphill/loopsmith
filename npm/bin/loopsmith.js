#!/usr/bin/env node
'use strict';
// Thin launcher. The real binary is fetched by scripts/install.js at postinstall
// time and lives beside this file.
//
// `spawnSync` with `stdio: 'inherit'`, not `execFileSync`: loopsmith is
// interactive and long-running, and its exit code is a verdict that has to
// arrive unchanged. Signals matter too — a run that gets Ctrl-C should look
// killed to the shell, not like a wrapper that exited 0.
const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');

const exe = path.join(
  __dirname,
  process.platform === 'win32' ? 'loopsmith.exe' : 'loopsmith-bin'
);

if (!fs.existsSync(exe)) {
  console.error(
    '[loopsmith] the binary is not installed.\n' +
    '[loopsmith] the postinstall download may have failed, or this platform has no\n' +
    '[loopsmith] prebuilt binary. Build from source instead:\n' +
    '[loopsmith]   cargo install loopsmith'
  );
  process.exit(127);
}

const r = spawnSync(exe, process.argv.slice(2), { stdio: 'inherit' });
if (r.error) {
  console.error(`[loopsmith] could not run ${exe}: ${r.error.message}`);
  process.exit(126);
}
// A signalled child has a null status. Re-raising on this process is what makes
// the shell see the same death the child had.
if (r.signal) {
  process.kill(process.pid, r.signal);
}
process.exit(r.status === null ? 1 : r.status);
