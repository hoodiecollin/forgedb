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

An EMPTY result from serena is not proof of absence, and a non-empty one can still be
partial. find_symbol reads a syntactic index and DOES see cfg-excluded items, but
find_referencing_symbols needs semantic resolution and reports ZERO CALLERS rather than
an error for anything the host build's cfg excludes — so a cfg-gated or feature-gated
symbol reads as dead code. Cross-check with ast-grep, which parses every file regardless
of cfg, before concluding a symbol is absent OR that nothing calls it.

Text search is still right for things with no AST node — string literals, error copy,
config keys, Cargo.toml version lines, .pm-playbook/backlog/. Scope those to a non-code
glob (e.g. glob: "*.toml"). Generated code held inside quote! bodies also has no reachable
AST node; search it as text.
EOF
exit 2
