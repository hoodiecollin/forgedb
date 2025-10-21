# ForgeDB Publishing Guide

**Last Updated:** October 2025

Complete guide for releasing and publishing ForgeDB crates to crates.io.

## Table of Contents

- [Overview](#overview)
- [Release Checklist](#release-checklist)
- [Version Bumping](#version-bumping)
- [Publishing to crates.io](#publishing-to-cratesio)
- [GitHub Releases](#github-releases)
- [Changelog Format](#changelog-format)
- [Rollback Procedures](#rollback-procedures)

---

## Overview

### Release Types

**Public Crates** (published to crates.io):
- `forgedb-types`
- `forgedb-storage`
- `forgedb-wal`
- `forgedb-crud-api`
- `forgedb-query-params`
- `forgedb-query-optimization`
- `forgedb-fulltext`
- `forgedb-http-server`
- `forgedb-compaction`
- `forgedb-ffi`

**Internal Crates** (NOT published):
- `forgedb-parser`
- `forgedb-validation`
- `forgedb-watcher`
- `forgedb-migrations`
- `forgedb-lsp-server`
- `forgedb` (CLI binary)

### Version Policy

All public crates are versioned together using [Semantic Versioning 2.0.0](https://semver.org/):

**MAJOR.MINOR.PATCH**
- **MAJOR**: Breaking API changes
- **MINOR**: New features, backward compatible
- **PATCH**: Bug fixes, backward compatible

**Pre-1.0 Policy:**
- Currently at `0.x.x` (pre-1.0)
- MINOR bumps MAY include breaking changes
- PATCH bumps are backward compatible

**Post-1.0 Policy:**
- MAJOR bumps for breaking changes
- MINOR bumps for new features (every 2-3 months)
- PATCH bumps for bug fixes (as needed)

---

## Release Checklist

### Pre-Release (1-2 weeks before)

- [ ] **Feature freeze**: No new features, only bug fixes
- [ ] **Update dependencies**: `cargo update` and test
- [ ] **Run full test suite**: `cargo test --all`
- [ ] **Run clippy**: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] **Check documentation**: `cargo doc --all --no-deps`
- [ ] **Test examples**: Run all examples and verify they work
- [ ] **Update CHANGELOG.md**: Document all changes since last release
- [ ] **Update version in docs**: Check all documentation for version references
- [ ] **Review open issues**: Address critical bugs
- [ ] **Review PRs**: Merge or defer pending pull requests

### Version Bump

- [ ] **Decide version number**: MAJOR.MINOR.PATCH
- [ ] **Update Cargo.toml files**: Bump version in all public crates
- [ ] **Update dependency versions**: Internal dependencies reference new version
- [ ] **Update README.md**: Version badges and installation instructions
- [ ] **Update documentation**: Any version-specific references
- [ ] **Commit version bump**: `git commit -m "chore: bump version to X.Y.Z"`

### Pre-Publish Verification

- [ ] **Clean build**: `cargo clean && cargo build --release`
- [ ] **Run tests**: `cargo test --all --release`
- [ ] **Test installation**: Create test project and verify dependency resolution
- [ ] **Check crate contents**: `cargo package --list` for each crate
- [ ] **Dry-run publish**: `cargo publish --dry-run` for each crate
- [ ] **Verify documentation builds**: Check docs.rs compatibility

### Publishing

- [ ] **Publish crates**: In dependency order (see below)
- [ ] **Verify on crates.io**: Check each crate page
- [ ] **Verify documentation**: Check docs.rs for each crate
- [ ] **Test installation**: `cargo new test && cargo add forgedb-storage`

### Post-Release

- [ ] **Create Git tag**: `git tag -a vX.Y.Z -m "Release X.Y.Z"`
- [ ] **Push tag**: `git push origin vX.Y.Z`
- [ ] **Create GitHub release**: With changelog and binaries
- [ ] **Announce release**: Blog post, Twitter, Discord, etc.
- [ ] **Update project board**: Close milestone, create next one
- [ ] **Monitor issues**: Watch for bug reports

---

## Version Bumping

### Manual Version Bump

**1. Determine new version:**
```bash
# Current version
CURRENT_VERSION="0.1.0"

# New version (choose one)
NEW_VERSION="0.1.1"  # Patch release
NEW_VERSION="0.2.0"  # Minor release
NEW_VERSION="1.0.0"  # Major release
```

**2. Update Cargo.toml files:**

Update version in all public crates:
```bash
# List all public crate Cargo.toml files
find crates -name Cargo.toml | grep -E "(storage|wal|crud-api|http-server|query-params|query-optimization|fulltext|compaction|ffi|types)" | xargs -I {} sed -i "s/version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" {}
```

**3. Update internal dependencies:**

Update dependency versions in Cargo.toml files:
```bash
# Update forgedb-storage dependency references
find crates -name Cargo.toml -exec sed -i "s/forgedb-storage = { version = \"$CURRENT_VERSION\"/forgedb-storage = { version = \"$NEW_VERSION\"/" {} \;

# Repeat for all public crates
```

**4. Update README.md:**
```markdown
<!-- Update version badge -->
[![Crates.io](https://img.shields.io/crates/v/forgedb-storage.svg)](https://crates.io/crates/forgedb-storage)

<!-- Update installation instruction -->
```toml
[dependencies]
forgedb-storage = "0.2.0"  # Update version here
```
```

**5. Verify changes:**
```bash
# Check all version changes
git diff | grep "version ="

# Ensure Cargo.lock is updated
cargo check

# Verify it compiles
cargo build --all
```

**6. Commit version bump:**
```bash
git add -A
git commit -m "chore: bump version to $NEW_VERSION"
```

### Automated Version Bump Script

Create `scripts/bump-version.sh`:
```bash
#!/bin/bash
set -e

if [ -z "$1" ]; then
    echo "Usage: $0 <new-version>"
    exit 1
fi

NEW_VERSION=$1
CURRENT_VERSION=$(grep '^version =' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

echo "Bumping version from $CURRENT_VERSION to $NEW_VERSION"

# Public crates
PUBLIC_CRATES=(
    "types"
    "storage"
    "wal"
    "crud-api"
    "query-params"
    "query-optimization"
    "fulltext"
    "http-server"
    "compaction"
    "ffi"
)

# Update version in each crate
for crate in "${PUBLIC_CRATES[@]}"; do
    CARGO_TOML="crates/$crate/Cargo.toml"
    if [ -f "$CARGO_TOML" ]; then
        echo "Updating $CARGO_TOML"
        sed -i "s/version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"
    fi
done

# Update dependencies
for crate in "${PUBLIC_CRATES[@]}"; do
    find crates -name Cargo.toml -exec sed -i \
        "s/forgedb-$crate = { version = \"$CURRENT_VERSION\"/forgedb-$crate = { version = \"$NEW_VERSION\"/" {} \;
done

# Update root Cargo.toml
sed -i "s/version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml

# Update Cargo.lock
cargo check

echo "Version bump complete. Review changes with 'git diff'"
```

**Usage:**
```bash
chmod +x scripts/bump-version.sh
./scripts/bump-version.sh 0.2.0
```

---

## Publishing to crates.io

### Prerequisites

**1. Crates.io Account:**
- Create account at [crates.io](https://crates.io)
- Get API token from [crates.io/me](https://crates.io/me)

**2. Configure Cargo:**
```bash
cargo login <your-api-token>
```

**3. Verify Ownership:**
Ensure you have owner/co-owner permissions for all crates.

### Publishing Order

**IMPORTANT**: Publish in dependency order to avoid failures.

**Order:**
```
1. forgedb-types (no dependencies)
2. forgedb-wal (no ForgeDB dependencies)
3. forgedb-storage (depends on: types, wal)
4. forgedb-crud-api (depends on: storage)
5. forgedb-query-params (no ForgeDB dependencies)
6. forgedb-query-optimization (depends on: storage, types)
7. forgedb-fulltext (no ForgeDB dependencies)
8. forgedb-compaction (no ForgeDB dependencies)
9. forgedb-http-server (no ForgeDB dependencies)
10. forgedb-ffi (depends on: storage, types)
```

### Publishing Script

Create `scripts/publish.sh`:
```bash
#!/bin/bash
set -e

# Publish crates in dependency order
CRATES=(
    "types"
    "wal"
    "storage"
    "crud-api"
    "query-params"
    "query-optimization"
    "fulltext"
    "compaction"
    "http-server"
    "ffi"
)

for crate in "${CRATES[@]}"; do
    echo "================================================"
    echo "Publishing forgedb-$crate..."
    echo "================================================"
    
    cd "crates/$crate"
    
    # Dry run first
    cargo publish --dry-run
    
    # Publish
    cargo publish
    
    cd ../..
    
    # Wait for crates.io to index
    echo "Waiting 30 seconds for crates.io to index..."
    sleep 30
done

echo "================================================"
echo "All crates published successfully!"
echo "================================================"
```

**Usage:**
```bash
chmod +x scripts/publish.sh
./scripts/publish.sh
```

### Manual Publishing

**For each crate (in order):**

```bash
cd crates/types

# 1. Verify package contents
cargo package --list

# 2. Dry-run (test without publishing)
cargo publish --dry-run

# 3. Publish
cargo publish

# 4. Wait for crates.io to index (30 seconds)
sleep 30

# 5. Move to next crate
cd ../wal
# Repeat...
```

### Verification

**After publishing each crate:**

```bash
# Check crate page
open https://crates.io/crates/forgedb-storage

# Verify documentation builds
open https://docs.rs/forgedb-storage

# Test installation
cargo new /tmp/test-install
cd /tmp/test-install
cargo add forgedb-storage --vers "0.2.0"
cargo build
```

### Troubleshooting Publishing

**Issue: "crate not found" for dependency**
- Wait longer for crates.io to index previous crate
- Verify previous crate was published successfully

**Issue: Documentation fails to build**
- Check for missing documentation features
- Verify all dependencies are available
- Test locally: `cargo doc --no-deps`

**Issue: Package too large**
- Check `.cargo_vcs_info.json` size
- Exclude unnecessary files in `Cargo.toml`:
  ```toml
  [package]
  exclude = ["tests/fixtures/*", "benches/*"]
  ```

**Issue: Yanking a release**
```bash
# Yank specific version (prevents new projects from using it)
cargo yank --vers 0.2.0 forgedb-storage

# Unyank if needed
cargo yank --undo --vers 0.2.0 forgedb-storage
```

---

## GitHub Releases

### Creating a Release

**1. Create Git Tag:**
```bash
# Tag the commit
git tag -a v0.2.0 -m "Release 0.2.0"

# Push tag
git push origin v0.2.0
```

**2. Create Release on GitHub:**

Go to: `https://github.com/yourusername/forgedb/releases/new`

**Release Template:**
```markdown
## ForgeDB v0.2.0

Release date: YYYY-MM-DD

### What's New

- Feature 1 description
- Feature 2 description

### Breaking Changes

- Breaking change 1 (if any)
- Migration guide: [link]

### Bug Fixes

- Fix 1 (#123)
- Fix 2 (#124)

### Documentation

- Updated guides
- New examples

### Installation

**Cargo.toml:**
```toml
[dependencies]
forgedb-storage = "0.2.0"
forgedb-http-server = "0.2.0"
```

**CLI:**
```bash
cargo install forgedb-cli --version 0.2.0
```

### Full Changelog

See [CHANGELOG.md](./CHANGELOG.md) for complete changes.

### Checksums

- forgedb-cli-linux: `sha256:...`
- forgedb-cli-macos: `sha256:...`
- forgedb-cli-windows: `sha256:...`
```

**3. Attach Binaries:**

Build release binaries for each platform:
```bash
# Linux
cargo build --release --bin forgedb
tar -czf forgedb-cli-v0.2.0-x86_64-linux.tar.gz -C target/release forgedb

# macOS
cargo build --release --target x86_64-apple-darwin --bin forgedb
tar -czf forgedb-cli-v0.2.0-x86_64-macos.tar.gz -C target/x86_64-apple-darwin/release forgedb

# Windows
cargo build --release --target x86_64-pc-windows-msvc --bin forgedb
zip forgedb-cli-v0.2.0-x86_64-windows.zip target/x86_64-pc-windows-msvc/release/forgedb.exe
```

**4. Generate Checksums:**
```bash
sha256sum forgedb-cli-*.tar.gz forgedb-cli-*.zip > checksums.txt
```

### Automated Release (GitHub Actions)

Create `.github/workflows/release.yml`:
```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - os: macos-latest
            target: x86_64-apple-darwin
          - os: windows-latest
            target: x86_64-pc-windows-msvc
    
    steps:
      - uses: actions/checkout@v3
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable
          target: ${{ matrix.target }}
      
      - name: Build
        run: cargo build --release --target ${{ matrix.target }}
      
      - name: Package
        shell: bash
        run: |
          cd target/${{ matrix.target }}/release
          if [ "${{ matrix.os }}" = "windows-latest" ]; then
            7z a ../../../forgedb-cli-${{ github.ref_name }}-${{ matrix.target }}.zip forgedb.exe
          else
            tar czf ../../../forgedb-cli-${{ github.ref_name }}-${{ matrix.target }}.tar.gz forgedb
          fi
      
      - name: Upload Artifact
        uses: actions/upload-artifact@v3
        with:
          name: forgedb-cli-${{ matrix.target }}
          path: forgedb-cli-*
  
  release:
    name: Create Release
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Download Artifacts
        uses: actions/download-artifact@v3
      
      - name: Create Release
        uses: softprops/action-gh-release@v1
        with:
          files: forgedb-cli-*/*
          body_path: CHANGELOG.md
          draft: false
          prerelease: false
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

---

## Changelog Format

### CHANGELOG.md Structure

Follow [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) format:

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Feature in development (#PR)

### Changed
- Upcoming change (#PR)

## [0.2.0] - 2025-10-20

### Added
- Full-text search support (#101)
- SIMD query optimization (#102)
- Response caching in HTTP server (#103)

### Changed
- Improved error messages in parser (#104)
- Updated Axum to 0.8 (#105)

### Fixed
- Fixed memory leak in compaction (#106)
- Corrected index corruption issue (#107)

### Security
- Updated rustls to fix CVE-2023-XXXX (#108)

## [0.1.0] - 2025-09-15

### Added
- Initial release
- Columnar storage engine
- Write-Ahead Log
- HTTP server infrastructure
- Basic CRUD operations

[Unreleased]: https://github.com/yourusername/forgedb/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/yourusername/forgedb/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/yourusername/forgedb/releases/tag/v0.1.0
```

### Changelog Categories

**Added**: New features
```markdown
### Added
- Full-text search with ranking (#101)
- New `forgedb-ffi` crate for C bindings (#102)
```

**Changed**: Changes to existing functionality
```markdown
### Changed
- Improved query optimizer performance by 2x (#103)
- Updated API to use `Result<T, Error>` instead of `Option<T>` (#104)
```

**Deprecated**: Soon-to-be removed features
```markdown
### Deprecated
- `old_method()` in favor of `new_method()` (#105)
- Will be removed in v1.0.0
```

**Removed**: Removed features
```markdown
### Removed
- Removed deprecated `legacy_api` module (#106)
```

**Fixed**: Bug fixes
```markdown
### Fixed
- Fixed race condition in WAL compaction (#107)
- Corrected off-by-one error in column indexing (#108)
```

**Security**: Security fixes
```markdown
### Security
- Fixed SQL injection vulnerability in query parser (#109)
- Updated dependencies to fix known CVEs (#110)
```

---

## Rollback Procedures

### Yanking a Release

If a critical bug is discovered:

**1. Yank the problematic version:**
```bash
cargo yank --vers 0.2.0 forgedb-storage
```

This prevents NEW projects from using it, but existing projects can still access it.

**2. Publish a patch release:**
```bash
# Fix the issue
# Bump version to 0.2.1
./scripts/bump-version.sh 0.2.1

# Publish fixed version
./scripts/publish.sh
```

**3. Update GitHub release:**
- Mark release as "Pre-release" or add warning
- Link to fixed version

### Reverting a Git Tag

If you need to remove a tag:
```bash
# Delete local tag
git tag -d v0.2.0

# Delete remote tag
git push origin :refs/tags/v0.2.0
```

### Emergency Hotfix Process

**1. Create hotfix branch:**
```bash
git checkout -b hotfix/0.2.1 v0.2.0
```

**2. Apply fix:**
```bash
# Make minimal changes to fix critical issue
git commit -m "fix: critical security issue"
```

**3. Bump version:**
```bash
./scripts/bump-version.sh 0.2.1
```

**4. Publish:**
```bash
./scripts/publish.sh
```

**5. Merge back:**
```bash
git checkout main
git merge hotfix/0.2.1
git push origin main
```

---

## Post-Release Checklist

### Immediate (Same Day)

- [ ] Verify all crates published successfully
- [ ] Check docs.rs for all crates
- [ ] Test installation: `cargo add forgedb-storage@0.2.0`
- [ ] Create GitHub release with binaries
- [ ] Update project website (if exists)
- [ ] Post announcement on social media

### Short-term (1-3 Days)

- [ ] Monitor GitHub issues for bug reports
- [ ] Check crates.io download stats
- [ ] Respond to community feedback
- [ ] Update examples to use new version
- [ ] Write blog post about release (optional)

### Long-term (1-2 Weeks)

- [ ] Evaluate release success
- [ ] Plan next release
- [ ] Update roadmap
- [ ] Document lessons learned

---

## Additional Resources

- [Semantic Versioning](https://semver.org/)
- [Keep a Changelog](https://keepachangelog.com/)
- [Cargo Book - Publishing](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [Architecture Documentation](./ARCHITECTURE.md)
- [Contributing Guide](./CONTRIBUTING.md)

---

**Questions?** Open an issue or contact maintainers.
