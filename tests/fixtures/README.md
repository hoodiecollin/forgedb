# Recorded cargo artifact streams

`cargo_stream_native.jsonl` and `cargo_stream_wasm.jsonl` are the **verbatim
stdout** of two real `cargo build --message-format=json-render-diagnostics`
runs. They are the fixture for `src/commands/build/driver.rs::parse_artifacts`,
which reads exactly this stream in production.

They are recordings, not models. A hand-written stream is a claim about what
cargo emits, and the claim is what `parse_artifacts` is trying to be right
about — so testing against one proves only that the parser agrees with its
author. Three things in these files contradict what a careful author would have
written by hand:

* `json-render-diagnostics` puts **no `compiler-message` on stdout at all** —
  diagnostics are rendered to stderr. The stream is artifacts and nothing else,
  plus `build-script-executed` and `build-finished`.
* A build script's `compiler-artifact` message carries **`"executable": null`**
  (cargo 1.96); the build script path arrives only in `filenames`.
* A `staticlib`+`cdylib`+`rlib` lib target reports **three** filenames and *no*
  `.rmeta`, while a plain `rlib` lib target reports **two** — the `.rlib` and an
  `.rmeta` under `deps/`.

## Re-recording

`./tests/fixtures/record_cargo_stream.sh` builds a dep-free four-package
workspace at `/tmp/forgedb-cargo-stream` and rewrites both files' sources there.
Copy the two `*.json` outputs over the `*.jsonl` fixtures. The workspace is
built at that fixed path on purpose: the recorded absolute paths are asserted
on, so the recording location is part of the fixture.

The workspace is shaped to cover the cases the parser discriminates:

| Package | Directory | Why |
|---|---|---|
| `blog-h-core` | `blog-h-core/` | dir name == package name, so cargo **abbreviates** the `package_id` to `…/blog-h-core#0.1.0` with no name in the fragment |
| `blog-h-ffi` | `ffi/` | dir name != package name → the `…/ffi#blog-h-ffi@0.1.0` form; three crate types in one target |
| `blog-h-server` | `server/` | a bin **with a build script**, so the stream carries a `custom-build` artifact too |
| `blog-h-wasm` | `replica/` | `crate-type = ["cdylib"]` on `wasm32-unknown-unknown` → a `.wasm` file |
