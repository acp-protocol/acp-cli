#!/usr/bin/env node

const { execFileSync } = require('child_process');
const path = require('path');

// Map Node.js platform/arch to npm package names
const PLATFORMS = {
  'darwin-arm64': '@acp-protocol/cli-darwin-arm64',
  'darwin-x64': '@acp-protocol/cli-darwin-x64',
  'linux-x64': '@acp-protocol/cli-linux-x64-gnu',
  'linux-arm64': '@acp-protocol/cli-linux-arm64-gnu',
  'win32-x64': '@acp-protocol/cli-win32-x64',
};

// Musl fallbacks for Linux (Alpine, etc.)
const MUSL_FALLBACKS = {
  'linux-x64': '@acp-protocol/cli-linux-x64-musl',
  'linux-arm64': '@acp-protocol/cli-linux-arm64-musl',
};

const platformKey = `${process.platform}-${process.arch}`;
let pkg = PLATFORMS[platformKey];

if (!pkg) {
  console.error(`Unsupported platform: ${platformKey}`);
  console.error('Supported platforms: darwin-arm64, darwin-x64, linux-x64, linux-arm64, win32-x64');
  process.exit(1);
}

/**
 * Try to resolve the binary path from a package
 */
function tryResolve(packageName) {
  try {
    const pkgPath = require.resolve(`${packageName}/package.json`);
    const pkgDir = path.dirname(pkgPath);
    const binName = process.platform === 'win32' ? 'acp.exe' : 'acp';
    const binaryPath = path.join(pkgDir, 'bin', binName);

    // Check if binary exists
    const fs = require('fs');
    if (fs.existsSync(binaryPath)) {
      return binaryPath;
    }
    return null;
  } catch {
    return null;
  }
}

// Try primary platform package
let binaryPath = tryResolve(pkg);

// Try musl fallback for Linux if glibc package not available
if (!binaryPath && MUSL_FALLBACKS[platformKey]) {
  binaryPath = tryResolve(MUSL_FALLBACKS[platformKey]);
}

if (!binaryPath) {
  console.error(`Could not find ACP binary for ${platformKey}`);
  console.error('');
  console.error('Try reinstalling:');
  console.error('  npm install -g @acp-protocol/cli');
  console.error('');
  console.error('Or install via other methods:');
  console.error('  cargo install acp');
  console.error('  brew install acp-protocol/tap/acp');
  process.exit(1);
}

// Execute the binary with all passed arguments
try {
  execFileSync(binaryPath, process.argv.slice(2), { stdio: 'inherit' });
} catch (error) {
  // If the binary exited with a code, use that code
  if (error.status !== undefined) {
    process.exit(error.status);
  }
  // Otherwise re-throw for unexpected errors
  throw error;
}
