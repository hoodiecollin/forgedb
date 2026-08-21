#!/usr/bin/env bash
# Record REAL cargo --message-format=json artifact streams for the driver's
# parse_artifacts() fixtures. Dep-free workspace so this is offline + fast.
set -euo pipefail
W=/tmp/forgedb-cargo-stream
rm -rf "$W"
mkdir -p "$W/blog-h-core/src" "$W/ffi/src" "$W/server/src" "$W/replica/src"

cat > "$W/Cargo.toml" <<'EOF'
[workspace]
resolver = "3"
members = ["blog-h-core", "ffi", "server", "replica"]
EOF

# package name == last path segment -> cargo abbreviates the package_id
cat > "$W/blog-h-core/Cargo.toml" <<'EOF'
[package]
name = "blog-h-core"
version = "0.1.0"
edition = "2024"

[lib]
name = "blog_h_core"
crate-type = ["rlib"]
EOF
# the unused binding makes cargo emit a real `compiler-message`
cat > "$W/blog-h-core/src/lib.rs" <<'EOF'
pub fn v() -> u32 {
    let unused_on_purpose = 7;
    1
}
EOF

# package name != last path segment -> cargo emits the `#name@version` form
cat > "$W/ffi/Cargo.toml" <<'EOF'
[package]
name = "blog-h-ffi"
version = "0.1.0"
edition = "2024"

[lib]
name = "blog_h_ffi"
crate-type = ["staticlib", "cdylib", "rlib"]
EOF
echo 'pub fn v() -> u32 { 2 }' > "$W/ffi/src/lib.rs"

# a bin WITH a build script: the build script reports its own compiler-artifact
cat > "$W/server/Cargo.toml" <<'EOF'
[package]
name = "blog-h-server"
version = "0.1.0"
edition = "2024"
build = "build.rs"

[[bin]]
name = "blog-h-server"
path = "src/main.rs"
EOF
echo 'fn main() { println!("cargo::rerun-if-changed=build.rs"); }' > "$W/server/build.rs"
echo 'fn main() { println!("hi"); }' > "$W/server/src/main.rs"

# the wasm arm: a cdylib that lands as a `.wasm`
cat > "$W/replica/Cargo.toml" <<'EOF'
[package]
name = "blog-h-wasm"
version = "0.1.0"
edition = "2024"

[lib]
name = "blog_h_wasm"
crate-type = ["cdylib"]
EOF
echo 'pub fn v() -> u32 { 3 }' > "$W/replica/src/lib.rs"

cd "$W"
CARGO_TARGET_DIR="$W/target" cargo build --release \
  -p blog-h-core -p blog-h-ffi -p blog-h-server \
  --message-format=json-render-diagnostics \
  > "$W/native.json" 2> "$W/native.stderr"
CARGO_TARGET_DIR="$W/target" cargo build --release \
  --target wasm32-unknown-unknown -p blog-h-wasm \
  --message-format=json-render-diagnostics \
  > "$W/wasm.json" 2> "$W/wasm.stderr"
wc -l "$W/native.json" "$W/wasm.json"
