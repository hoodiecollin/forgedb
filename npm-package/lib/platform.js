// Platform detection for ForgeDB binaries
import { platform, arch } from 'os';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export function getPlatform() {
  const platformMap = {
    darwin: 'macos',
    linux: 'linux',
    win32: 'windows',
  };

  const archMap = {
    x64: 'x64',
    arm64: 'arm64',
  };

  const os = platformMap[platform()];
  const cpu = archMap[arch()];

  if (!os || !cpu) {
    throw new Error(
      `Unsupported platform: ${platform()}-${arch()}\n` +
      `ForgeDB supports: macOS (x64/arm64), Linux (x64), Windows (x64)`
    );
  }

  return { os, cpu };
}

export function getBinaryName() {
  const { os, cpu } = getPlatform();
  const ext = os === 'windows' ? '.exe' : '';
  return `forgedb-${os}-${cpu}${ext}`;
}

export function getBinaryPath() {
  const binaryName = getBinaryName();
  return join(__dirname, '..', 'binaries', binaryName);
}
