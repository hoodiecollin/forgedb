#!/usr/bin/env node

// Post-install script - verify binary exists and set permissions
import { existsSync, chmodSync } from 'fs';
import { getBinaryPath, getPlatform } from '../lib/platform.js';

export async function postInstall() {
  try {
    const { os, cpu } = getPlatform();
    const binaryPath = getBinaryPath();

    console.log(`[ForgeDB] Setting up for ${os}-${cpu}...`);

    // Check if binary exists
    if (!existsSync(binaryPath)) {
      console.error('\n⚠️  ForgeDB binary not found!');
      console.error('Expected location:', binaryPath);
      console.error('\nThis package currently only includes binaries for macOS arm64.');
      console.error('For other platforms, please build from source:');
      console.error('  git clone https://github.com/forgedb/forgedb');
      console.error('  cd forgedb && cargo build --release');
      process.exit(1);
    }

    // Make executable (Unix-like systems)
    if (os !== 'windows') {
      try {
        chmodSync(binaryPath, 0o755);
      } catch (err) {
        console.warn('⚠️  Could not set executable permissions:', err.message);
        console.warn('You may need to run: chmod +x', binaryPath);
      }
    }

    console.log('✓ ForgeDB CLI installed successfully');
    console.log('\nTry it out:');
    console.log('  forgedb --version');
    console.log('  forgedb --help');

  } catch (err) {
    console.error('Failed to setup ForgeDB:', err.message);
    process.exit(1);
  }
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  postInstall();
}
