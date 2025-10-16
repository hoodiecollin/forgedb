# NPM Package Plan: ForgeDB CLI

**Goal**: Package ForgeDB as an NPM module with CLI binary for easy installation and usage
**Estimated Time**: 1-2 days
**Created**: 2025-10-15

---

## Overview

Create an NPM package that:
1. Installs the ForgeDB CLI globally or locally
2. Provides the `forgedb` command
3. Downloads/bundles the correct Rust binary for the platform
4. Works cross-platform (macOS, Linux, Windows)

---

## Package Structure

```
forgedb/
├── package.json           # NPM package configuration
├── bin/
│   └── forgedb.js         # CLI entry point (Node.js wrapper)
├── lib/
│   ├── download.js        # Download Rust binary on install
│   ├── platform.js        # Platform detection
│   └── runner.js          # Execute Rust binary
├── scripts/
│   └── postinstall.js     # Post-install script
├── binaries/              # Pre-built binaries (optional)
│   ├── forgedb-macos-arm64
│   ├── forgedb-macos-x64
│   ├── forgedb-linux-x64
│   └── forgedb-windows-x64.exe
├── README.md
└── .npmignore
```

---

## Implementation Tasks

### Task 1: Create package.json (1 hour)

```json
{
  "name": "@forgedb/cli",
  "version": "0.1.0",
  "description": "ForgeDB - Schema-first database with code generation",
  "type": "module",
  "bin": {
    "forgedb": "./bin/forgedb.js"
  },
  "scripts": {
    "postinstall": "node scripts/postinstall.js",
    "build": "cargo build --release",
    "test": "node test/cli.test.js"
  },
  "files": [
    "bin",
    "lib",
    "scripts",
    "binaries",
    "README.md"
  ],
  "keywords": [
    "database",
    "schema",
    "codegen",
    "cli",
    "forgedb"
  ],
  "engines": {
    "node": ">=18.0.0"
  },
  "repository": {
    "type": "git",
    "url": "https://github.com/forgedb/forgedb"
  },
  "author": "ForgeDB Team",
  "license": "MIT",
  "dependencies": {
    "cross-spawn": "^7.0.3"
  },
  "devDependencies": {
    "pkg-install": "^1.0.0"
  },
  "optionalDependencies": {
    "@forgedb/cli-darwin-arm64": "0.1.0",
    "@forgedb/cli-darwin-x64": "0.1.0",
    "@forgedb/cli-linux-x64": "0.1.0",
    "@forgedb/cli-win32-x64": "0.1.0"
  },
  "os": [
    "darwin",
    "linux",
    "win32"
  ],
  "cpu": [
    "x64",
    "arm64"
  ]
}
```

---

### Task 2: Create Platform Detection (1 hour)

```javascript
// lib/platform.js
import { platform, arch } from 'os';

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
    throw new Error(`Unsupported platform: ${platform()}-${arch()}`);
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
  return new URL(`../binaries/${binaryName}`, import.meta.url).pathname;
}
```

---

### Task 3: Create CLI Wrapper (2 hours)

```javascript
#!/usr/bin/env node
// bin/forgedb.js

import { spawn } from 'cross-spawn';
import { getBinaryPath } from '../lib/platform.js';
import { existsSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

async function main() {
  try {
    const binaryPath = getBinaryPath();

    // Check if binary exists
    if (!existsSync(binaryPath)) {
      console.error('ForgeDB binary not found. Running post-install...');
      const { postInstall } = await import('../scripts/postinstall.js');
      await postInstall();
    }

    // Forward all arguments to the Rust binary
    const args = process.argv.slice(2);

    const child = spawn(binaryPath, args, {
      stdio: 'inherit',
      windowsHide: true,
    });

    child.on('exit', (code) => {
      process.exit(code || 0);
    });

    child.on('error', (err) => {
      console.error('Failed to execute ForgeDB:', err.message);
      process.exit(1);
    });

  } catch (err) {
    console.error('Error:', err.message);
    console.error('\nPlease report this issue at: https://github.com/forgedb/forgedb/issues');
    process.exit(1);
  }
}

main();
```

---

### Task 4: Post-Install Script (2 hours)

Option A: Download from GitHub releases

```javascript
// scripts/postinstall.js
import { createWriteStream, chmodSync } from 'fs';
import { mkdir } from 'fs/promises';
import { dirname } from 'path';
import { pipeline } from 'stream/promises';
import { getBinaryPath, getPlatform } from '../lib/platform.js';

const GITHUB_REPO = 'forgedb/forgedb';
const VERSION = process.env.npm_package_version || '0.1.0';

export async function postInstall() {
  try {
    const { os, cpu } = getPlatform();
    const binaryPath = getBinaryPath();

    // Create binaries directory
    await mkdir(dirname(binaryPath), { recursive: true });

    console.log(`Downloading ForgeDB ${VERSION} for ${os}-${cpu}...`);

    const url = `https://github.com/${GITHUB_REPO}/releases/download/v${VERSION}/forgedb-${os}-${cpu}${os === 'windows' ? '.exe' : ''}`;

    const response = await fetch(url);

    if (!response.ok) {
      throw new Error(`Failed to download: ${response.statusText}`);
    }

    // Download and save binary
    await pipeline(
      response.body,
      createWriteStream(binaryPath)
    );

    // Make executable (Unix-like systems)
    if (os !== 'windows') {
      chmodSync(binaryPath, 0o755);
    }

    console.log('✓ ForgeDB installed successfully');

  } catch (err) {
    console.error('Failed to install ForgeDB:', err.message);
    console.error('\nYou can manually download the binary from:');
    console.error(`https://github.com/${GITHUB_REPO}/releases`);
    process.exit(1);
  }
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  postInstall();
}
```

Option B: Bundle binaries with package (simpler for testing)

```javascript
// scripts/postinstall.js
import { copyFileSync, chmodSync, existsSync } from 'fs';
import { getBinaryPath, getPlatform } from '../lib/platform.js';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export async function postInstall() {
  try {
    const binaryPath = getBinaryPath();

    // Check if binary already exists
    if (existsSync(binaryPath)) {
      console.log('✓ ForgeDB binary already installed');
      return;
    }

    const { os } = getPlatform();

    // Make executable (Unix-like systems)
    if (os !== 'windows') {
      chmodSync(binaryPath, 0o755);
    }

    console.log('✓ ForgeDB installed successfully');

  } catch (err) {
    console.error('Failed to setup ForgeDB:', err.message);
    process.exit(1);
  }
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  postInstall();
}
```

---

### Task 5: Build Script (1 hour)

Create script to build binaries for all platforms:

```javascript
// scripts/build-all.js
import { execSync } from 'child_process';
import { mkdirSync, copyFileSync } from 'fs';
import { join } from 'path';

const platforms = [
  { os: 'macos', arch: 'arm64', target: 'aarch64-apple-darwin' },
  { os: 'macos', arch: 'x64', target: 'x86_64-apple-darwin' },
  { os: 'linux', arch: 'x64', target: 'x86_64-unknown-linux-gnu' },
  { os: 'windows', arch: 'x64', target: 'x86_64-pc-windows-gnu' },
];

function buildBinary(platform) {
  console.log(`Building for ${platform.os}-${platform.arch}...`);

  // Add target if not installed
  execSync(`rustup target add ${platform.target}`, { stdio: 'inherit' });

  // Build
  execSync(`cargo build --release --target ${platform.target}`, {
    stdio: 'inherit',
  });

  // Copy binary
  mkdirSync('binaries', { recursive: true });

  const ext = platform.os === 'windows' ? '.exe' : '';
  const src = join(
    'target',
    platform.target,
    'release',
    `forgedb${ext}`
  );
  const dest = join(
    'binaries',
    `forgedb-${platform.os}-${platform.arch}${ext}`
  );

  copyFileSync(src, dest);
  console.log(`✓ Built ${dest}`);
}

// Build for all platforms
for (const platform of platforms) {
  try {
    buildBinary(platform);
  } catch (err) {
    console.error(`Failed to build for ${platform.os}-${platform.arch}:`, err.message);
  }
}

console.log('\n✓ All binaries built successfully');
```

**Usage**:
```bash
node scripts/build-all.js
```

---

### Task 6: Create README (1 hour)

```markdown
# ForgeDB CLI

Schema-first database with automatic code generation.

## Installation

### Global Installation (Recommended)

\`\`\`bash
npm install -g @forgedb/cli
\`\`\`

### Local Installation

\`\`\`bash
npm install --save-dev @forgedb/cli
\`\`\`

## Usage

### Initialize a new project

\`\`\`bash
forgedb init
\`\`\`

### Define your schema

Create \`schema.forge\`:

\`\`\`forge
User {
  id: +uuid
  email: string @unique
  name: string
  created_at: timestamp
}

Post {
  id: +uuid
  title: string
  content: text
  author: User
  published: bool
}
\`\`\`

### Generate code

\`\`\`bash
# Generate TypeScript SDK
forgedb generate typescript

# Generate REST API
forgedb generate api

# Generate OpenAPI spec
forgedb generate openapi
\`\`\`

### Start the server

\`\`\`bash
forgedb serve
\`\`\`

## Commands

\`\`\`
forgedb init                    # Initialize new project
forgedb generate <target>       # Generate code (typescript, api, openapi)
forgedb serve                   # Start database server
forgedb migrate                 # Run migrations
forgedb validate                # Validate schema
forgedb version                 # Show version
\`\`\`

## Requirements

- Node.js >= 18.0.0
- One of: macOS (x64/arm64), Linux (x64), Windows (x64)

## Troubleshooting

### Binary not found

If you see "ForgeDB binary not found", try:

\`\`\`bash
npm rebuild @forgedb/cli
\`\`\`

### Permission denied (macOS/Linux)

\`\`\`bash
chmod +x ./node_modules/.bin/forgedb
\`\`\`

## Development

### Build from source

\`\`\`bash
git clone https://github.com/forgedb/forgedb
cd forgedb
cargo build --release
\`\`\`

### Local testing

\`\`\`bash
npm link
\`\`\`

## License

MIT
\`\`\`

---

### Task 7: Local Testing Setup (2 hours)

#### Step 1: Build the binary

```bash
# Build release binary for current platform
cd /Users/collin/Projects/_/kitchen-sink
cargo build --release

# Copy to npm package binaries directory
mkdir -p npm-package/binaries
cp target/release/forgedb npm-package/binaries/forgedb-macos-arm64
chmod +x npm-package/binaries/forgedb-macos-arm64
```

#### Step 2: Set up NPM package structure

```bash
cd npm-package

# Create directory structure
mkdir -p bin lib scripts

# Create all the files (package.json, bin/forgedb.js, etc.)
# ... (copy content from tasks above)
```

#### Step 3: Link to local path

```bash
# From npm-package directory
npm link

# Verify it's linked
which forgedb
# Should show: /usr/local/bin/forgedb or similar

# Test it
forgedb --version
forgedb --help
```

#### Step 4: Create realistic test project

```bash
# Create test project
mkdir -p ~/forgedb-test-project
cd ~/forgedb-test-project

# Initialize
forgedb init

# This should create:
# - schema.forge
# - forgedb.toml (config)
# - .gitignore

# Create a realistic schema
cat > schema.forge << 'EOF'
# E-commerce Database Schema

User {
  id: +uuid
  email: string @unique
  name: string
  password_hash: string
  created_at: timestamp
  updated_at: timestamp

  orders: [Order]
  cart: Cart?
}

Product {
  id: +uuid
  name: string
  description: text
  price: f64
  stock: i32
  category: Category
  created_at: timestamp

  images: [ProductImage]
  reviews: [Review]
}

Category {
  id: +uuid
  name: string @unique
  slug: string @unique

  products: [Product]
}

ProductImage {
  id: +uuid
  product: Product
  url: string
  alt_text: string?
  order: i32
}

Order {
  id: +uuid
  user: User
  status: string  # pending, paid, shipped, delivered, cancelled
  total: f64
  created_at: timestamp
  updated_at: timestamp

  items: [OrderItem]
}

OrderItem {
  id: +uuid
  order: Order
  product: Product
  quantity: i32
  price: f64  # Price at time of order
}

Cart {
  id: +uuid
  user: User
  updated_at: timestamp

  items: [CartItem]
}

CartItem {
  id: +uuid
  cart: Cart
  product: Product
  quantity: i32
}

Review {
  id: +uuid
  product: Product
  user: User
  rating: i32  # 1-5
  comment: text?
  created_at: timestamp
}
EOF

# Validate schema
forgedb validate schema.forge

# Generate TypeScript SDK
forgedb generate typescript --output ./generated

# Generate REST API
forgedb generate api --output ./api

# Generate OpenAPI spec
forgedb generate openapi --output ./openapi.yaml

# Start server
forgedb serve --port 3000
```

---

## Testing Checklist

### Installation Tests
- [ ] `npm link` works without errors
- [ ] `forgedb` command is available globally
- [ ] Binary has correct permissions (executable)
- [ ] Works on current platform (macOS arm64)

### Command Tests
- [ ] `forgedb --version` shows correct version
- [ ] `forgedb --help` shows help text
- [ ] `forgedb init` creates project structure
- [ ] `forgedb validate` validates schema
- [ ] `forgedb generate typescript` generates code
- [ ] `forgedb generate api` generates API
- [ ] `forgedb generate openapi` generates spec
- [ ] `forgedb serve` starts server

### Real Project Tests
- [ ] Can create realistic e-commerce schema
- [ ] Schema validation catches errors
- [ ] TypeScript SDK generates correctly
- [ ] API generates correctly
- [ ] Server starts and responds
- [ ] Can insert/query data via API

### Error Handling Tests
- [ ] Invalid schema shows clear error
- [ ] Missing binary shows helpful message
- [ ] Invalid command shows help
- [ ] Permission errors handled gracefully

---

## Package Structure (Final)

```
npm-package/
├── package.json              # NPM configuration
├── README.md                 # User documentation
├── .npmignore               # Files to exclude from npm
│
├── bin/
│   └── forgedb.js           # CLI entry point (Node.js wrapper)
│
├── lib/
│   ├── platform.js          # Platform detection utilities
│   └── runner.js            # Binary execution helper
│
├── scripts/
│   ├── postinstall.js       # Post-install setup
│   └── build-all.js         # Build binaries for all platforms
│
└── binaries/                # Pre-built binaries
    ├── forgedb-macos-arm64
    ├── forgedb-macos-x64
    ├── forgedb-linux-x64
    └── forgedb-windows-x64.exe
```

---

## .npmignore

```
# Development
*.rs
Cargo.toml
Cargo.lock
target/
.cargo/

# Tests
test/
tests/
*.test.js

# Build scripts (keep postinstall)
scripts/build-all.js

# Git
.git/
.gitignore

# IDE
.vscode/
.idea/

# Misc
*.log
node_modules/
```

---

## Publishing Checklist

Before publishing to NPM:

- [ ] Version bumped in package.json
- [ ] CHANGELOG.md updated
- [ ] All binaries built for all platforms
- [ ] Binaries tested on each platform
- [ ] README.md accurate and complete
- [ ] License file included
- [ ] GitHub repository linked
- [ ] .npmignore properly configured

```bash
# Test package locally
npm pack
tar -xvzf forgedb-cli-0.1.0.tgz
cd package
npm link

# Publish to NPM
npm publish --access public
```

---

## Timeline

| Task | Duration |
|------|----------|
| Create package.json | 1 hour |
| Platform detection | 1 hour |
| CLI wrapper | 2 hours |
| Post-install script | 2 hours |
| Build script | 1 hour |
| README documentation | 1 hour |
| Local testing setup | 2 hours |
| **Total** | **10 hours (~1-2 days)** |

---

## Next Steps

1. Create npm-package directory structure
2. Build binary for current platform
3. Implement all scripts (package.json, bin/forgedb.js, etc.)
4. Test with npm link
5. Create realistic test project
6. Verify all commands work
7. Document any issues

---

**Status**: 📋 Planning
**Last Updated**: 2025-10-15
**Ready to implement**: Yes
