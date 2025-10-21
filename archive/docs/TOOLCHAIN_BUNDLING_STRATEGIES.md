# Toolchain Bundling Strategies

**Goal**: Provide flexible toolchain management that works for everyone - from zero dependencies to full dev environment
**Created**: 2025-10-16

---

## Philosophy

ForgeDB should work seamlessly in three environments:
1. **Production/End Users**: Zero external dependencies - everything bundled
2. **Developers with Toolchains**: Use system Bun/Rust if available (faster, smaller)
3. **Mixed Environments**: Flexible combination of bundled and system tools

---

## Configuration System

### forgedb.toml Configuration

```toml
[toolchain]
# Strategy: "auto" | "system" | "bundled" | "hybrid"
strategy = "auto"

[toolchain.bun]
# Use system Bun if available, fallback to bundled
mode = "auto"  # "auto" | "system" | "bundled"
# Minimum version required (if using system)
min_version = "1.2.0"
# Path to system bun (optional, auto-detected if not specified)
path = "/usr/local/bin/bun"

[toolchain.rust]
# Use system Rust if available, fallback to bundled
mode = "auto"  # "auto" | "system" | "bundled"
# Minimum version required (if using system)
min_version = "1.75.0"
# Path to cargo (optional, auto-detected if not specified)
cargo_path = "/usr/local/bin/cargo"

[toolchain.bundled]
# Extract bundled tools to this directory
extract_dir = ".forgedb/toolchains"
# Re-use extracted tools across projects (in ~/.forgedb/)
global_cache = true
# Verify checksums of extracted tools
verify_checksums = true
```

### Environment Variables (Override Config)

```bash
# Force specific strategy
FORGEDB_TOOLCHAIN_STRATEGY=system  # override strategy

# Per-tool overrides
FORGEDB_BUN_MODE=bundled          # force bundled Bun
FORGEDB_RUST_MODE=system          # force system Rust

# Custom paths
FORGEDB_BUN_PATH=/custom/path/to/bun
FORGEDB_CARGO_PATH=/custom/path/to/cargo

# Disable bundled tools entirely (fail if system not available)
FORGEDB_NO_BUNDLED=true
```

---

## Strategy Implementations

### Strategy 1: AUTO (Recommended Default)

**Behavior**: Smart detection with graceful fallbacks

```rust
// crates/toolchain/src/strategy/auto.rs

pub struct AutoStrategy {
    config: ToolchainConfig,
}

impl AutoStrategy {
    pub fn resolve_bun(&self) -> Result<BunRuntime> {
        // 1. Try system Bun first
        if let Ok(system_bun) = self.detect_system_bun() {
            if self.meets_min_version(&system_bun, &self.config.bun.min_version) {
                ui::info(&format!(
                    "Using system Bun {} at {}",
                    system_bun.version, system_bun.path
                ));
                return Ok(BunRuntime::System(system_bun));
            } else {
                ui::warning(&format!(
                    "System Bun {} is below minimum version {}",
                    system_bun.version, self.config.bun.min_version
                ));
            }
        }

        // 2. Fallback to bundled Bun
        ui::info("Using bundled Bun runtime");
        self.extract_bundled_bun()
    }

    pub fn resolve_rust(&self) -> Result<RustToolchain> {
        // 1. Try system Rust first
        if let Ok(system_rust) = self.detect_system_rust() {
            if self.meets_min_version(&system_rust, &self.config.rust.min_version) {
                ui::info(&format!(
                    "Using system Rust {} at {}",
                    system_rust.version, system_rust.cargo_path
                ));
                return Ok(RustToolchain::System(system_rust));
            } else {
                ui::warning(&format!(
                    "System Rust {} is below minimum version {}",
                    system_rust.version, self.config.rust.min_version
                ));
            }
        }

        // 2. Fallback to bundled Rust
        ui::info("Using bundled Rust toolchain");
        self.extract_bundled_rust()
    }

    fn detect_system_bun(&self) -> Result<SystemBun> {
        // Check custom path first
        if let Some(path) = &self.config.bun.path {
            return self.probe_bun(path);
        }

        // Check PATH
        if let Ok(output) = Command::new("bun").arg("--version").output() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let path = which::which("bun")?;
            return Ok(SystemBun { version, path });
        }

        Err(ToolchainError::NotFound("bun"))
    }

    fn detect_system_rust(&self) -> Result<SystemRust> {
        // Similar to Bun detection
        if let Ok(output) = Command::new("cargo").arg("--version").output() {
            // Parse "cargo 1.75.0 (1234abc 2024-01-01)"
            let version_output = String::from_utf8_lossy(&output.stdout);
            let version = version_output
                .split_whitespace()
                .nth(1)
                .ok_or(ToolchainError::ParseError)?
                .to_string();

            let cargo_path = which::which("cargo")?;
            let rustc_path = which::which("rustc")?;

            return Ok(SystemRust {
                version,
                cargo_path,
                rustc_path,
            });
        }

        Err(ToolchainError::NotFound("cargo"))
    }
}
```

**Use Cases**:
- ✅ Developer laptop with Bun/Rust installed → uses system tools (fast)
- ✅ CI environment with no tools → uses bundled (works)
- ✅ Mixed environment (Rust but no Bun) → uses system Rust, bundled Bun
- ✅ Production deployment → uses bundled (no dependencies)

---

### Strategy 2: SYSTEM (Fail Fast)

**Behavior**: Only use system tools, fail if not available

```rust
// crates/toolchain/src/strategy/system.rs

pub struct SystemStrategy {
    config: ToolchainConfig,
}

impl SystemStrategy {
    pub fn resolve_bun(&self) -> Result<BunRuntime> {
        let system_bun = self.detect_system_bun()
            .map_err(|_| ToolchainError::Required {
                tool: "bun",
                message: "System Bun required but not found.\n\
                         Install from: https://bun.sh\n\
                         Or use: strategy = 'auto' in forgedb.toml"
            })?;

        if !self.meets_min_version(&system_bun, &self.config.bun.min_version) {
            return Err(ToolchainError::VersionMismatch {
                tool: "bun",
                required: self.config.bun.min_version.clone(),
                found: system_bun.version.clone(),
            });
        }

        Ok(BunRuntime::System(system_bun))
    }

    pub fn resolve_rust(&self) -> Result<RustToolchain> {
        // Similar strict system-only logic
    }
}
```

**Use Cases**:
- ✅ Corporate environments with approved toolchain versions
- ✅ Security-conscious setups (no embedded binaries)
- ✅ Developers who want explicit control
- ✅ Environments with custom Rust patches/builds

**Config Example**:
```toml
[toolchain]
strategy = "system"

[toolchain.bun]
min_version = "1.2.22"  # Exact version they have

[toolchain.rust]
min_version = "1.75.0"
```

---

### Strategy 3: BUNDLED (Self-Contained)

**Behavior**: Always use bundled tools, ignore system

```rust
// crates/toolchain/src/strategy/bundled.rs

pub struct BundledStrategy {
    config: ToolchainConfig,
    embedded: EmbeddedToolchains,
}

impl BundledStrategy {
    pub fn resolve_bun(&self) -> Result<BunRuntime> {
        // Always extract and use bundled Bun
        let extracted = self.extract_bundled_bun()?;
        ui::info(&format!(
            "Using bundled Bun runtime at {}",
            extracted.path.display()
        ));
        Ok(BunRuntime::Bundled(extracted))
    }

    pub fn resolve_rust(&self) -> Result<RustToolchain> {
        // Always extract and use bundled Rust
        let extracted = self.extract_bundled_rust()?;
        ui::info(&format!(
            "Using bundled Rust toolchain at {}",
            extracted.cargo_path.display()
        ));
        Ok(RustToolchain::Bundled(extracted))
    }

    fn extract_bundled_bun(&self) -> Result<BundledBun> {
        let platform = detect_platform()?;
        let extract_dir = self.get_extract_dir();
        let bun_path = extract_dir.join("bun");

        // Check if already extracted
        if bun_path.exists() && self.verify_integrity(&bun_path) {
            return Ok(BundledBun { path: bun_path });
        }

        // Extract from embedded bytes
        ui::info("Extracting bundled Bun runtime...");
        let bun_bytes = self.embedded.get_bun(&platform)?;
        fs::write(&bun_path, bun_bytes)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&bun_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&bun_path, perms)?;
        }

        ui::success("Bundled Bun runtime extracted");
        Ok(BundledBun { path: bun_path })
    }

    fn extract_bundled_rust(&self) -> Result<BundledRust> {
        // Similar to Bun extraction
        // Extracts: cargo, rustc, rust-std, and necessary libs
    }

    fn get_extract_dir(&self) -> PathBuf {
        if self.config.bundled.global_cache {
            // Extract to ~/.forgedb/toolchains/
            // Shared across all projects
            dirs::home_dir()
                .unwrap()
                .join(".forgedb")
                .join("toolchains")
        } else {
            // Extract to project-local .forgedb/toolchains/
            Path::new(&self.config.bundled.extract_dir).to_path_buf()
        }
    }

    fn verify_integrity(&self, path: &Path) -> bool {
        if !self.config.bundled.verify_checksums {
            return true;
        }

        // Verify SHA256 checksum matches embedded checksum
        let computed = compute_sha256(path).ok()?;
        let expected = self.embedded.get_checksum(path.file_name()?.to_str()?)?;
        computed == expected
    }
}
```

**Use Cases**:
- ✅ Production deployments (guaranteed consistency)
- ✅ Isolated testing environments
- ✅ Systems without package managers
- ✅ Air-gapped/offline environments (if bundled in advance)

---

### Strategy 4: HYBRID (Maximum Flexibility)

**Behavior**: Mix and match based on per-tool configuration

```rust
// crates/toolchain/src/strategy/hybrid.rs

pub struct HybridStrategy {
    config: ToolchainConfig,
}

impl HybridStrategy {
    pub fn resolve_bun(&self) -> Result<BunRuntime> {
        match self.config.bun.mode.as_str() {
            "system" => SystemStrategy::new(&self.config).resolve_bun(),
            "bundled" => BundledStrategy::new(&self.config).resolve_bun(),
            "auto" => AutoStrategy::new(&self.config).resolve_bun(),
            mode => Err(ToolchainError::InvalidMode(mode.to_string())),
        }
    }

    pub fn resolve_rust(&self) -> Result<RustToolchain> {
        match self.config.rust.mode.as_str() {
            "system" => SystemStrategy::new(&self.config).resolve_rust(),
            "bundled" => BundledStrategy::new(&self.config).resolve_rust(),
            "auto" => AutoStrategy::new(&self.config).resolve_rust(),
            mode => Err(ToolchainError::InvalidMode(mode.to_string())),
        }
    }
}
```

**Use Cases**:
- ✅ Use system Rust (dev has it) + bundled Bun (don't have it)
- ✅ Use bundled Rust (specific version) + system Bun (already installed)
- ✅ Fine-grained control per project

**Config Example**:
```toml
[toolchain]
strategy = "hybrid"

[toolchain.bun]
mode = "bundled"  # Always use bundled Bun (consistent version)

[toolchain.rust]
mode = "system"   # Use system Rust (dev has custom setup)
min_version = "1.75.0"
```

---

## Bundled Rust Toolchain Design

### Challenges
- Rust toolchain is MUCH larger than Bun (~400MB vs ~90MB)
- Need cargo, rustc, rust-std, and core libs
- Cross-compilation dependencies

### Solution: Minimal Rust Bundle

```rust
// crates/embedded-toolchains/src/rust.rs

pub struct MinimalRustToolchain {
    // Core binaries
    cargo: &'static [u8],
    rustc: &'static [u8],

    // Standard library (for target platform)
    rust_std: &'static [u8],

    // Minimal libs needed for compilation
    libs: Vec<(&'static str, &'static [u8])>,

    // Metadata
    version: &'static str,
    platform: &'static str,
}

impl MinimalRustToolchain {
    pub fn extract(&self, dest: &Path) -> Result<()> {
        let bin_dir = dest.join("bin");
        let lib_dir = dest.join("lib");

        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(&lib_dir)?;

        // Extract cargo
        let cargo_path = bin_dir.join("cargo");
        fs::write(&cargo_path, self.cargo)?;
        make_executable(&cargo_path)?;

        // Extract rustc
        let rustc_path = bin_dir.join("rustc");
        fs::write(&rustc_path, self.rustc)?;
        make_executable(&rustc_path)?;

        // Extract std lib
        extract_archive(self.rust_std, &lib_dir)?;

        // Extract necessary libs
        for (name, bytes) in &self.libs {
            fs::write(lib_dir.join(name), bytes)?;
        }

        // Create wrapper scripts that set up environment
        self.create_wrappers(&bin_dir, &lib_dir)?;

        Ok(())
    }

    fn create_wrappers(&self, bin_dir: &Path, lib_dir: &Path) -> Result<()> {
        // Create cargo wrapper that sets RUSTC, RUST_STD, etc.
        let cargo_wrapper = format!(
            r#"#!/bin/bash
export RUSTC="{}/rustc"
export RUST_LIB="{}"
exec "{}/cargo" "$@"
"#,
            bin_dir.display(),
            lib_dir.display(),
            bin_dir.display()
        );

        fs::write(bin_dir.join("cargo-wrapper"), cargo_wrapper)?;
        make_executable(&bin_dir.join("cargo-wrapper"))?;

        Ok(())
    }
}
```

### Size Optimization for Rust

**Technique 1: Compress with zstd**
```rust
// Compress Rust toolchain with zstd (60-70% reduction)
const CARGO_COMPRESSED: &[u8] = include_bytes!("../../../bundled/cargo.zst");

fn decompress_cargo() -> Result<Vec<u8>> {
    zstd::decode_all(CARGO_COMPRESSED)
        .map_err(|e| ToolchainError::Decompression(e))
}
```

**Technique 2: Lazy Extraction**
```rust
// Only extract what's needed
pub struct LazyRustToolchain {
    // Extract cargo first (10MB)
    // Only extract full toolchain if user runs `forgedb build --rust-from-source`
}
```

**Technique 3: Download on Demand**
```rust
// For larger Rust bundles, download on first use
pub struct OnDemandRustToolchain {
    // Include checksums and URLs in binary
    // Download and verify on first `forgedb build`
    // Cache in ~/.forgedb/toolchains/rust-{version}/
}
```

### Recommended Approach: Hybrid Rust Bundling

```toml
[toolchain.rust]
mode = "auto"  # Default: use system if available

# If bundled mode is needed
[toolchain.rust.bundled]
# "inline" - embedded in binary (~400MB compressed to ~150MB)
# "download" - download on first use (smaller binary)
# "minimal" - only cargo wrapper, downloads toolchain on demand
distribution = "minimal"

# Where to download from (if using download mode)
download_url = "https://forgedb.dev/toolchains/rust-{version}-{platform}.tar.zst"
```

---

## Clever Detection & Caching Strategies

### Strategy 1: Environment Fingerprinting

```rust
// crates/toolchain/src/fingerprint.rs

pub struct EnvironmentFingerprint {
    pub has_rustup: bool,
    pub has_cargo: bool,
    pub has_bun: bool,
    pub has_node: bool,
    pub has_npm: bool,
    pub detected_rust_version: Option<String>,
    pub detected_bun_version: Option<String>,
}

impl EnvironmentFingerprint {
    pub fn detect() -> Self {
        // Quick parallel detection
        let (has_rustup, has_cargo) = rayon::join(
            || which::which("rustup").is_ok(),
            || which::which("cargo").is_ok(),
        );

        let (has_bun, has_node) = rayon::join(
            || which::which("bun").is_ok(),
            || which::which("node").is_ok(),
        );

        // ... detect versions

        Self {
            has_rustup,
            has_cargo,
            has_bun,
            has_node,
            has_npm,
            detected_rust_version,
            detected_bun_version,
        }
    }

    pub fn recommend_strategy(&self) -> &'static str {
        match (self.has_cargo, self.has_bun) {
            (true, true) => "system",   // Both available
            (false, false) => "bundled", // Neither available
            _ => "hybrid",               // Mixed
        }
    }

    pub fn cache_key(&self) -> String {
        // Generate cache key based on environment
        format!(
            "rust:{}_bun:{}",
            self.detected_rust_version.as_deref().unwrap_or("none"),
            self.detected_bun_version.as_deref().unwrap_or("none")
        )
    }
}
```

### Strategy 2: Intelligent First-Run Setup

```rust
// First time running ForgeDB in a project
pub fn interactive_setup() -> Result<()> {
    ui::header("🔧", "ForgeDB Toolchain Setup");

    let fingerprint = EnvironmentFingerprint::detect();

    println!("Detected environment:");
    println!("  Rust:  {}", fingerprint.detected_rust_version
        .as_deref().unwrap_or("not found"));
    println!("  Bun:   {}", fingerprint.detected_bun_version
        .as_deref().unwrap_or("not found"));

    let recommended = fingerprint.recommend_strategy();
    println!("\nRecommended strategy: {}", recommended);

    // Interactive prompts
    let strategy = prompt_select(
        "Choose toolchain strategy:",
        &["auto (recommended)", "system", "bundled", "hybrid"],
    )?;

    // Generate forgedb.toml with recommended settings
    let config = generate_config(&strategy, &fingerprint)?;
    fs::write("forgedb.toml", config)?;

    ui::success("Created forgedb.toml with toolchain configuration");
    Ok(())
}
```

### Strategy 3: Per-Project Toolchain Caching

```rust
// .forgedb/toolchain.lock
{
  "version": "1",
  "strategy": "auto",
  "resolved": {
    "bun": {
      "type": "system",
      "path": "/usr/local/bin/bun",
      "version": "1.2.22",
      "verified_at": "2025-10-16T16:00:00Z"
    },
    "rust": {
      "type": "bundled",
      "extract_path": ".forgedb/toolchains/rust-1.75.0",
      "version": "1.75.0",
      "extracted_at": "2025-10-16T16:00:00Z",
      "checksum": "sha256:abc123..."
    }
  }
}
```

```rust
pub struct ToolchainLock {
    // Cache resolved toolchain paths
    // Avoid re-detection on every command
    // Invalidate if versions change
}

impl ToolchainLock {
    pub fn is_valid(&self) -> bool {
        // Check if cached resolution is still valid
        match &self.resolved.bun.type_ {
            ToolchainType::System => {
                // Verify system tool still exists and version matches
                self.verify_system_tool(&self.resolved.bun)
            }
            ToolchainType::Bundled => {
                // Verify extracted tool still exists and checksum matches
                self.verify_bundled_tool(&self.resolved.bun)
            }
        }
    }
}
```

### Strategy 4: Global Toolchain Cache

```rust
// ~/.forgedb/cache/
//   toolchains/
//     bun-1.2.22-darwin-arm64/
//     rust-1.75.0-darwin-arm64/
//   metadata.json

pub struct GlobalToolchainCache {
    cache_dir: PathBuf,  // ~/.forgedb/cache/toolchains/
}

impl GlobalToolchainCache {
    pub fn get_or_extract(&self, tool: &str, version: &str, platform: &str)
        -> Result<PathBuf>
    {
        let cache_key = format!("{}-{}-{}", tool, version, platform);
        let cached_path = self.cache_dir.join(&cache_key);

        if cached_path.exists() && self.verify_cached(&cached_path) {
            ui::info(&format!("Using cached {} from {}", tool, cached_path.display()));
            return Ok(cached_path);
        }

        // Extract to cache
        ui::info(&format!("Extracting {} to global cache...", tool));
        self.extract_to_cache(tool, version, platform, &cached_path)?;

        Ok(cached_path)
    }

    pub fn cleanup_old_versions(&self, keep_latest: usize) -> Result<()> {
        // Clean up old cached toolchains to save disk space
        // Keep only the N most recent versions
    }
}
```

---

## Command-Specific Optimizations

### `forgedb init` - Minimal Requirements
```rust
// Don't require full toolchain for init
// Just create project structure
pub fn init(project_name: &str) -> Result<()> {
    // No toolchain needed - just file operations
    create_project_structure(project_name)?;

    // Detect environment and recommend strategy
    let fingerprint = EnvironmentFingerprint::detect();
    create_forgedb_toml_with_recommended_strategy(fingerprint)?;

    Ok(())
}
```

### `forgedb generate` - Bun Optional
```rust
// Bun only needed if generating TypeScript/React
pub fn generate(target: &str) -> Result<()> {
    match target {
        "rust" => {
            // No Bun needed, no Rust needed (just codegen)
            generate_rust_code()?;
        }
        "typescript" | "react" => {
            // Might need Bun for type checking (optional)
            let bun = resolve_bun_optional()?;
            generate_typescript_code(bun)?;
        }
        "all" => {
            // Generate all, but Bun still optional
        }
    }
    Ok(())
}
```

### `forgedb build` - Full Toolchain Required
```rust
pub fn build(options: BuildOptions) -> Result<()> {
    // This is where we need the full toolchain
    let toolchain = ToolchainResolver::new().resolve_all()?;

    if !options.no_db {
        // Need Rust
        let rust = toolchain.rust?;
        build_rust_database(&rust, &options)?;
    }

    if !options.no_api {
        // Need Bun
        let bun = toolchain.bun?;
        build_typescript_runtime(&bun, &options)?;
    }

    Ok(())
}
```

### `forgedb dev` - Lazy Loading
```rust
pub fn dev(options: DevOptions) -> Result<()> {
    // Start watching schema immediately
    // Lazy load toolchains as needed

    let watcher = create_schema_watcher(&options.schema)?;

    // Only load Bun when we need to run the server
    let bun = LazyCell::new(|| {
        ui::info("Loading Bun runtime...");
        ToolchainResolver::new().resolve_bun()
    });

    for event in watcher.events() {
        match event {
            Event::SchemaChanged => {
                regenerate_code()?;

                // Now we need Bun to restart server
                let bun = bun.force()?;
                restart_server(bun)?;
            }
        }
    }

    Ok(())
}
```

---

## Binary Size Comparison

### Current Approach (No Bundling)
```
forgedb binary:          2MB
User must install:
  - Bun:                90MB
  - Rust:              400MB
Total footprint:       492MB
```

### Bundled Everything (Naive)
```
forgedb binary with:
  - Rust database:       2MB
  - 4x Bun runtimes:    40MB (compressed: 20MB)
  - 4x Rust toolchain: 600MB (compressed: 200MB)
Total binary size:     220MB
```

### Smart Bundling (Recommended)
```
forgedb binary with:
  - Rust database:       2MB
  - 4x Bun runtimes:    40MB (compressed: 20MB)
  - Rust: on-demand download
Total binary size:      22MB

On first build (if no system Rust):
  - Download Rust:     150MB (cached in ~/.forgedb/)

Total with cache:      172MB (vs 492MB before)
```

### Minimal Distribution (NPM Package)
```
@forgedb/cli package:
  - CLI binary:          2MB
  - 4x Bun runtimes:    20MB (compressed)
  - Checksums/metadata:  1KB
Total NPM package:     22MB

Downloads Rust on-demand if needed (not in package)
```

---

## Implementation Priority

### Phase 1: Configuration System (Day 1)
- [ ] Create `crates/toolchain` crate
- [ ] Implement `forgedb.toml` parsing
- [ ] Environment variable overrides
- [ ] Environment fingerprinting

### Phase 2: Detection & Resolution (Day 2)
- [ ] System tool detection (Bun, Rust)
- [ ] Version checking
- [ ] AUTO strategy implementation
- [ ] Toolchain lock file

### Phase 3: Bundled Bun (Day 3)
- [ ] Embed compiled Bun runtimes
- [ ] Extraction logic
- [ ] Global cache support
- [ ] BUNDLED strategy for Bun

### Phase 4: Bundled Rust (Day 4-5)
- [ ] Minimal Rust toolchain design
- [ ] Compression with zstd
- [ ] On-demand download option
- [ ] BUNDLED strategy for Rust

### Phase 5: Integration & Testing (Day 6)
- [ ] Integrate with all commands
- [ ] Cross-platform testing
- [ ] Performance benchmarks
- [ ] Documentation

---

## Success Metrics

- [ ] `forgedb init` works with zero dependencies
- [ ] `forgedb generate` works without Bun/Rust
- [ ] `forgedb build` auto-detects system tools
- [ ] `forgedb build` falls back to bundled if needed
- [ ] Binary size under 25MB (Bun bundled, Rust on-demand)
- [ ] First build time < 30s with bundled toolchain
- [ ] Developer with system tools sees no slowdown
- [ ] Clear error messages when toolchain unavailable

---

## Future Enhancements

1. **Toolchain Update Management**
   ```bash
   forgedb toolchain update
   forgedb toolchain list
   forgedb toolchain install bun@1.3.0
   ```

2. **Project-Specific Toolchain Versions**
   ```toml
   [toolchain.bun]
   version = "1.2.22"  # Lock to specific version
   ```

3. **Custom Toolchain Sources**
   ```toml
   [toolchain.rust.bundled]
   source = "https://my-company.com/rust-toolchains/"
   ```

4. **Toolchain Verification**
   ```bash
   forgedb doctor  # Check all toolchains
   ```

5. **Offline Mode**
   ```bash
   forgedb bundle --offline  # Create fully offline package
   ```
