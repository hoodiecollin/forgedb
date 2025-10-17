# Bundled Bun Runtime Enhancement Plan

**Goal**: Make ForgeDB completely self-contained by bundling Bun and compiling TypeScript to standalone executables
**Estimated Time**: 2-3 days
**Created**: 2025-10-16

---

## Problem Statement

Currently, ForgeDB assumes developers have Bun installed in their environment:
- Runtime requires `bun` to be available in PATH
- TypeScript server/components require Bun runtime
- Adds external dependency that users must manage
- Version compatibility issues between user's Bun and ForgeDB's runtime

## Solution Overview

Bundle everything into a single self-contained executable:
1. **Compile TypeScript server to standalone binary** using `bun build --compile`
2. **Bundle Bun runtime** for platforms that need it
3. **Embed TypeScript executable into Rust binary** (similar to nginx embedding)
4. **Single `forgedb` binary** that contains both database and app server

---

## Architecture

### Current Architecture
```
User Project
├── schema.forge
├── forgedb.toml
├── data/ (database)
├── generated/ (TypeScript SDK, React components)
└── pages/ (user's React components)

External Dependencies:
- Bun runtime (must be installed)
- Node.js packages (react, react-dom)
```

### New Architecture
```
Single forgedb executable (5-10MB)
├── Rust database engine
├── Bundled TypeScript server (Bun compiled)
│   ├── FFI bindings
│   ├── React SSR runtime
│   └── Component renderer
└── Embedded assets

User Project (simplified)
├── schema.forge
├── forgedb.toml
├── data/ (database)
└── pages/ (user's React components)
```

---

## Implementation Tasks

### Phase 1: Bun Compilation Setup (4 hours)

#### Task 1.1: Create TypeScript Build Configuration
Create `runtime/bun/build.ts`:
```typescript
// Build configuration for standalone executable
import { $ } from "bun";

const targets = [
  "bun-darwin-arm64",
  "bun-darwin-x64",
  "bun-linux-x64",
  "bun-windows-x64"
];

for (const target of targets) {
  console.log(`Building for ${target}...`);

  await $`bun build ./src/server.ts \
    --compile \
    --target=${target} \
    --minify \
    --bytecode \
    --outfile=./dist/forgedb-server-${target}`;
}
```

#### Task 1.2: Update Server Entry Point
Modify `runtime/bun/src/server.ts` to:
- Accept configuration via environment variables
- Support embedded mode (no dynamic imports)
- Preload component registry from manifest
- Fallback to dynamic loading in dev mode

```typescript
// Detect if running as compiled executable
const IS_COMPILED = typeof Bun.main === 'string' && Bun.main.endsWith('.exe');

// In compiled mode, use manifest; in dev mode, use dynamic imports
const componentManifest = IS_COMPILED
  ? require('./component-manifest.json')
  : null;
```

#### Task 1.3: Component Manifest Generation
Create build step to generate component manifest:
```typescript
// scripts/generate-manifest.ts
// Scans pages/ directory and creates static manifest
{
  "components": {
    "user-card": "./pages/user/card/page.tsx",
    "user-profile": "./pages/user/profile/page.tsx"
  }
}
```

### Phase 2: Cross-Platform Bun Binaries (2 hours)

#### Task 2.1: Download Bun Release Binaries
Create script to download official Bun binaries:
```bash
scripts/download-bun-binaries.sh
```

Downloads:
- `bun-darwin-arm64` (macOS Apple Silicon)
- `bun-darwin-x64` (macOS Intel)
- `bun-linux-x64` (Linux x64)
- `bun-windows-x64.exe` (Windows x64)

Store in: `runtime/bun/binaries/`

#### Task 2.2: Add to Build Process
Update `forgedb build` command to:
1. Generate TypeScript code
2. Compile TypeScript server for all platforms
3. Store compiled executables in `target/runtime/`

### Phase 3: Embed TypeScript in Rust Binary (6 hours)

#### Task 3.1: Rust Embedding Using `include_bytes!`
Similar to nginx embedding, use Rust macros:

```rust
// crates/embedded-runtime/src/lib.rs

pub struct EmbeddedRuntime {
    platforms: HashMap<String, &'static [u8]>,
}

impl EmbeddedRuntime {
    pub fn new() -> Self {
        let mut platforms = HashMap::new();

        #[cfg(target_os = "macos")]
        #[cfg(target_arch = "aarch64")]
        platforms.insert(
            "darwin-arm64".to_string(),
            include_bytes!("../../../runtime/bun/dist/forgedb-server-darwin-arm64")
        );

        #[cfg(target_os = "macos")]
        #[cfg(target_arch = "x86_64")]
        platforms.insert(
            "darwin-x64".to_string(),
            include_bytes!("../../../runtime/bun/dist/forgedb-server-darwin-x64")
        );

        // ... other platforms

        Self { platforms }
    }

    pub fn extract_to(&self, platform: &str, dest: &Path) -> Result<()> {
        let binary = self.platforms.get(platform)
            .ok_or_else(|| anyhow!("Unsupported platform: {}", platform))?;

        fs::write(dest, binary)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(dest)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(dest, perms)?;
        }

        Ok(())
    }
}
```

#### Task 3.2: Runtime Extraction on First Run
Extract embedded runtime to user's project:

```rust
// On `forgedb build` or first `forgedb dev`
pub fn ensure_runtime_extracted() -> Result<PathBuf> {
    let runtime_dir = Path::new(".forgedb/runtime");
    let runtime_path = runtime_dir.join("forgedb-server");

    if !runtime_path.exists() {
        fs::create_dir_all(runtime_dir)?;

        let embedded = EmbeddedRuntime::new();
        let platform = detect_platform()?;
        embedded.extract_to(&platform, &runtime_path)?;
    }

    Ok(runtime_path)
}
```

#### Task 3.3: Spawn Runtime Server
Update server spawning logic:

```rust
// crates/cli/src/commands/dev.rs
pub fn spawn_server() -> Result<Child> {
    let runtime_path = ensure_runtime_extracted()?;

    let child = Command::new(runtime_path)
        .env("DB_MODE", "ffi")
        .env("FORGEDB_DATA", "./data")
        .env("PORT", "3001")
        .spawn()?;

    Ok(child)
}
```

### Phase 4: Build Pipeline Integration (4 hours)

#### Task 4.1: Update `forgedb build` Command
Enhance build command to compile everything:

```rust
// New build flow:
pub fn run(options: BuildOptions) -> Result<()> {
    // 1. Validate schema
    // 2. Generate code (Rust + TypeScript)

    // 3. Build TypeScript runtime (NEW)
    if !options.no_api {
        build_typescript_runtime()?;
    }

    // 4. Build Rust database
    if !options.no_db {
        build_rust_database(&options)?;
    }

    // 5. Create final package
    package_artifacts(&options)?;
}

fn build_typescript_runtime() -> Result<()> {
    ui::info("Compiling TypeScript server...");

    // Check if bun is available for building
    let has_bun = Command::new("bun").arg("--version").status().is_ok();

    if !has_bun {
        return Err(CliError::Build(
            "Bun is required to build TypeScript runtime.\n\
             Install from: https://bun.sh".to_string()
        ));
    }

    // Compile server
    let output = Command::new("bun")
        .args(&[
            "build",
            "generated/server.ts",
            "--compile",
            "--minify",
            "--bytecode",
            "--outfile=target/forgedb-server"
        ])
        .output()?;

    if !output.status.success() {
        return Err(CliError::Build(
            format!("TypeScript compilation failed:\n{}",
                String::from_utf8_lossy(&output.stderr))
        ));
    }

    ui::success("TypeScript server compiled");
    Ok(())
}
```

#### Task 4.2: Package Artifacts
Create final distributable package:

```rust
fn package_artifacts(options: &BuildOptions) -> Result<()> {
    let output_dir = options.output.as_deref().unwrap_or("dist");
    fs::create_dir_all(output_dir)?;

    // Copy database binary
    let db_binary = if options.release {
        "target/release/forgedb"
    } else {
        "target/debug/forgedb"
    };
    fs::copy(db_binary, format!("{}/forgedb", output_dir))?;

    // Copy TypeScript server
    fs::copy("target/forgedb-server", format!("{}/forgedb-server", output_dir))?;

    // Copy data directory
    copy_dir("data", format!("{}/data", output_dir))?;

    // Create run script
    create_run_script(output_dir)?;

    ui::success(&format!("Package created in {}/", output_dir));
    Ok(())
}
```

### Phase 5: NPM Package Updates (3 hours)

#### Task 5.1: Bundle Compiled Runtimes in NPM Package
Update NPM package structure:

```
npm-package/
├── binaries/
│   └── forgedb-macos-arm64 (Rust CLI)
├── runtime/
│   ├── forgedb-server-darwin-arm64 (Bun compiled)
│   ├── forgedb-server-darwin-x64
│   ├── forgedb-server-linux-x64
│   └── forgedb-server-windows-x64.exe
└── lib/
    └── runtime-loader.js (extracts and runs appropriate runtime)
```

#### Task 5.2: Runtime Loader
Create `lib/runtime-loader.js`:

```javascript
import { existsSync, chmodSync, writeFileSync } from 'fs';
import { join } from 'path';
import { spawn } from 'cross-spawn';
import { getPlatform } from './platform.js';

export function getServerRuntimePath() {
  const { os, cpu } = getPlatform();
  const runtimeName = `forgedb-server-${os}-${cpu}${os === 'windows' ? '.exe' : ''}`;

  // Return path to bundled runtime
  return join(__dirname, '..', 'runtime', runtimeName);
}

export function spawnServer(options = {}) {
  const runtimePath = getServerRuntimePath();

  if (!existsSync(runtimePath)) {
    throw new Error(`Runtime not found for platform: ${runtimePath}`);
  }

  // Ensure executable on Unix
  if (process.platform !== 'win32') {
    chmodSync(runtimePath, 0o755);
  }

  return spawn(runtimePath, [], {
    env: {
      ...process.env,
      DB_MODE: options.dbMode || 'ffi',
      FORGEDB_DATA: options.dataPath || './data',
      PORT: options.port || '3001',
    },
    stdio: 'inherit',
  });
}
```

### Phase 6: Testing & Optimization (4 hours)

#### Task 6.1: Test Suite
- Test compilation on all platforms
- Test runtime extraction and execution
- Test FFI bindings work with compiled server
- Test component rendering
- Test route handlers

#### Task 6.2: Size Optimization
- Minify TypeScript bundles
- Use bytecode compilation
- Strip debug symbols from Rust binary
- Compress embedded runtimes with `flate2`

#### Task 6.3: Documentation
- Update build documentation
- Document deployment process
- Add troubleshooting guide

---

## File Size Estimates

### Before (separate dependencies):
- Rust binary: 2MB
- User must install Bun: ~90MB
- Total user footprint: ~92MB

### After (bundled):
- Rust binary with embedded runtimes: 8-12MB
  - Rust database: 2MB
  - 4 Bun compiled servers @ 2-3MB each: 8-12MB (compressed: 4-6MB)
- Total user footprint: 8-12MB (or 6-8MB compressed)

### NPM Package:
- Before: ~2MB (just Rust binary)
- After: ~10-15MB (Rust + 4 platform runtimes)

---

## Benefits

1. **Zero External Dependencies**: Users don't need to install Bun
2. **Version Control**: Runtime version locked with ForgeDB version
3. **Faster Deployment**: Single binary to distribute
4. **Better UX**: `npm install @forgedb/cli` and you're done
5. **Consistent Behavior**: Same runtime across all environments
6. **Easier CI/CD**: No need to set up Bun in CI pipelines

---

## Risks & Mitigations

### Risk 1: Large Binary Size
**Mitigation**:
- Use compression for embedded runtimes
- Only extract needed platform at runtime
- Document size trade-offs

### Risk 2: Bun Compilation Limitations
**Mitigation**:
- Test thoroughly with dynamic imports
- Use component manifest for static analysis
- Keep dev mode with dynamic loading

### Risk 3: Cross-Platform Testing
**Mitigation**:
- Set up CI for all platforms
- Use GitHub Actions matrix builds
- Community testing before release

---

## Alternative Approaches Considered

### 1. Bundle Bun Source and Compile at Install
**Rejected**: Too slow, requires build tools on user machine

### 2. Download Bun at Runtime
**Rejected**: Network dependency, version management issues

### 3. Keep Bun as External Dependency
**Rejected**: Doesn't meet goal of self-contained distribution

### 4. Use Deno Instead
**Rejected**: Would require rewriting runtime, Bun FFI is faster

---

## Future Enhancements

1. **Plugin System**: Allow users to extend runtime with plugins
2. **Hot Reload**: Watch mode for components in development
3. **Multiple Runtimes**: Support both Bun and Deno
4. **Edge Deployment**: Compile for Cloudflare Workers, Deno Deploy
5. **Container Images**: Pre-built Docker images with bundled runtime

---

## Success Criteria

- [ ] User can install `@forgedb/cli` and run without installing Bun
- [ ] Single `forgedb build` command produces deployment-ready artifact
- [ ] Binary size under 15MB for all-in-one package
- [ ] No performance regression compared to unbundled version
- [ ] All existing features work with bundled runtime
- [ ] Clear documentation for deployment

---

## Timeline

- **Day 1**: Phase 1-2 (Bun compilation setup + cross-platform binaries)
- **Day 2**: Phase 3-4 (Rust embedding + build pipeline)
- **Day 3**: Phase 5-6 (NPM updates + testing)

---

## Implementation Checklist

### Immediate (Can Start Now)
- [ ] Create `runtime/bun/build.ts` build script
- [ ] Test `bun build --compile` with current server
- [ ] Download Bun binaries for all platforms
- [ ] Create component manifest generator

### Requires Planning
- [ ] Design embedding API in Rust
- [ ] Plan extraction strategy
- [ ] Design package structure

### Requires Testing
- [ ] Test on all platforms
- [ ] Benchmark performance
- [ ] Measure binary sizes
