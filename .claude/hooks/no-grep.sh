#!/usr/bin/env bash
set -uo pipefail

input=$(cat 2>/dev/null || true)
[ -n "$input" ] || exit 0

field() { jq -r "$1 // \"\"" <<<"$input" 2>/dev/null || printf ''; }

glob=$(field '.tool_input.glob')
path=$(field '.tool_input.path')
pattern=$(field '.tool_input.pattern')

[ -n "$pattern" ] || exit 0

case "$glob" in
  *.md|*.mdx|*.json|*.jsonc|*.yaml|*.yml|*.toml|*.sql|*.css|*.scss|*.txt|*.env*|*.lock|*.proto|*.sh)
    exit 0 ;;
esac

case "$path" in
  */docs/*|*/docs|*/.github/*|*/.pm-playbook/*|*/examples/*|*/migrations/*|*/vendor/*) exit 0 ;;
esac

[[ "$pattern" =~ ^[A-Za-z_][A-Za-z0-9_.:]*$ ]] || exit 0

cat >&2 <<EOF
"${pattern}" is identifier-shaped, and text search cannot tell a definition from a call site,
an import, a comment, or a same-named symbol in another module.

  Definition ............ serena: find_symbol
  Usages ................ serena: find_referencing_symbols
  File structure ........ serena: get_symbols_overview
  Structural pattern .... ast-grep run -p '<pattern>' -l rust|ts|tsx|go

An EMPTY result from serena is not proof of absence. rust-analyzer resolves ONE build
configuration and the host target. On a native host that hides every item under
#[cfg(target_arch = "wasm32")] — 5 sites across crates/storage/src/lib.rs (the facade's
web re-export), crates/wal/src/lib.rs (the in-memory WalManager) and
crates/storage-web/src/lib.rs. Small, but it is the substrate seam, so a miss there is
load-bearing. Cross-check with ast-grep, which parses every file regardless of cfg,
before reporting a symbol missing.

Text search is still right for things with no AST node — string literals, error copy,
config keys, Cargo.toml version lines, .pm-playbook/backlog/. Scope those to a non-code
glob (e.g. glob: "*.toml"). Generated code held inside quote! bodies also has no reachable
AST node; search it as text.
EOF
exit 2
