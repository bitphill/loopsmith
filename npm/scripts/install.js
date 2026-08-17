#!/usr/bin/env node
'use strict';
// Downloads the prebuilt `loopsmith` for this host from the GitHub release that
// matches this package's version, verifies it against the release's published
// SHA256SUMS, and installs it beside this script.
//
// The checksum step is the point. A postinstall script that pipes a downloaded
// binary onto disk unverified is a supply-chain hole with a progress bar, and the
// release workflow publishes SHA256SUMS precisely so this can refuse.
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { execFileSync } = require('child_process');

const pkg = require('../package.json');
const VERSION = pkg.version;
const REPO = 'bitphill/loopsmith';

// musl's ldd has no --version flag: it prints usage plus "musl libc" to stderr
// and exits non-zero, where glibc's answers and exits 0. Detecting this wrong
// yields a binary that dies with "not found" on a perfectly good libc.
function isMusl() {
  try {
    execFileSync('ldd', ['--version'], { stdio: ['ignore', 'pipe', 'pipe'] });
    return false;
  } catch (e) {
    return /musl/i.test(String(e.stderr || ''));
  }
}

function resolveTarget() {
  const { platform, arch } = process;
  if (platform === 'linux') {
    if (arch === 'x64') {
      return isMusl() ? 'x86_64-unknown-linux-musl' : 'x86_64-unknown-linux-gnu';
    }
    if (arch === 'arm64') return 'aarch64-unknown-linux-gnu';
  }
  if (platform === 'darwin') {
    if (arch === 'x64') return 'x86_64-apple-darwin';
    if (arch === 'arm64') return 'aarch64-apple-darwin';
  }
  if (platform === 'win32' && arch === 'x64') return 'x86_64-pc-windows-msvc';
  return null;
}

async function fetchBuffer(url) {
  const res = await fetch(url, { redirect: 'follow' });
  if (!res.ok) throw new Error(`HTTP ${res.status} fetching ${url}`);
  return Buffer.from(await res.arrayBuffer());
}

async function main() {
  const target = resolveTarget();
  const binDir = path.join(__dirname, '..', 'bin');
  const isWindows = process.platform === 'win32';
  const destExe = path.join(binDir, isWindows ? 'loopsmith.exe' : 'loopsmith-bin');

  if (!target) {
    console.warn(
      `[loopsmith] no prebuilt binary for ${process.platform}/${process.arch} in v${VERSION}.\n` +
      `[loopsmith] build from source instead: cargo install loopsmith`
    );
    return;
  }

  const ext = isWindows ? 'zip' : 'tar.gz';
  const asset = `loopsmith-v${VERSION}-${target}.${ext}`;
  const base = `https://github.com/${REPO}/releases/download/v${VERSION}`;

  console.log(`[loopsmith] downloading ${base}/${asset}`);
  const [archive, sumsText] = await Promise.all([
    fetchBuffer(`${base}/${asset}`),
    fetchBuffer(`${base}/SHA256SUMS`).then((b) => b.toString('utf8')),
  ]);

  const wantLine = sumsText.split('\n').find((l) => l.trim().endsWith(asset));
  if (!wantLine) throw new Error(`${asset} is not listed in the release SHA256SUMS`);
  const wantHash = wantLine.trim().split(/\s+/)[0];
  const gotHash = crypto.createHash('sha256').update(archive).digest('hex');
  if (gotHash !== wantHash) {
    throw new Error(`checksum mismatch for ${asset}: expected ${wantHash}, got ${gotHash}`);
  }

  const tmp = path.join(os.tmpdir(), asset);
  fs.writeFileSync(tmp, archive);
  fs.mkdirSync(binDir, { recursive: true });
  const member = isWindows ? 'loopsmith.exe' : 'loopsmith';
  try {
    if (isWindows) {
      // tar.exe has shipped with Windows 10 1803 and later and reads zips.
      execFileSync('tar', ['xf', tmp, '-C', binDir, member], { stdio: 'inherit' });
    } else {
      execFileSync('tar', ['xzf', tmp, '-C', binDir, member], { stdio: 'inherit' });
    }
    const extracted = path.join(binDir, member);
    if (extracted !== destExe) fs.renameSync(extracted, destExe);
    fs.chmodSync(destExe, 0o755);
  } finally {
    fs.unlinkSync(tmp);
  }
  console.log(`[loopsmith] installed ${destExe}`);
}

main().catch((err) => {
  console.error(`[loopsmith] postinstall failed: ${err.message}`);
  console.error('[loopsmith] build from source instead: cargo install loopsmith');
  // Non-fatal on purpose. A flaky download should not hard-fail an `npm install`
  // that may be installing twenty other things; bin/loopsmith.js reports a clear
  // error at run time if the binary never landed.
  process.exitCode = 0;
});
