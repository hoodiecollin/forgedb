#!/usr/bin/env node

// ForgeDB CLI wrapper - spawns the Rust binary with all arguments
import { spawn } from 'cross-spawn';
import { getBinaryPath } from '../lib/platform.js';
import { existsSync } from 'fs';

async function main() {
  try {
    const binaryPath = getBinaryPath();

    // Check if binary exists
    if (!existsSync(binaryPath)) {
      console.error('Error: ForgeDB binary not found.');
      console.error('Expected location:', binaryPath);
      console.error('\nPlease try reinstalling:');
      console.error('  npm install --force @forgedb/cli');
      console.error('\nOr report this issue at:');
      console.error('  https://github.com/forgedb/forgedb/issues');
      process.exit(1);
    }

    // Forward all arguments to the Rust binary
    const args = process.argv.slice(2);

    const child = spawn(binaryPath, args, {
      stdio: 'inherit',
      windowsHide: true,
      shell: false,
    });

    child.on('exit', (code, signal) => {
      if (signal) {
        process.kill(process.pid, signal);
      } else {
        process.exit(code || 0);
      }
    });

    child.on('error', (err) => {
      console.error('Failed to execute ForgeDB:', err.message);
      console.error('\nPlease check:');
      console.error('  1. Binary has execute permissions');
      console.error('  2. Your platform is supported');
      console.error('  3. Binary is not corrupted');
      console.error('\nTry reinstalling:');
      console.error('  npm install --force @forgedb/cli');
      process.exit(1);
    });

  } catch (err) {
    console.error('Error:', err.message);
    console.error('\nPlease report this issue at:');
    console.error('  https://github.com/forgedb/forgedb/issues');
    process.exit(1);
  }
}

main();
