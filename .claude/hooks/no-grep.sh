#!/usr/bin/env bash
set -uo pipefail

input=$(cat 2>/dev/null || true)
[ -n "$input" ] || exit 0

field() { jq -r "$1 // \"\"" <<<"$input" 2>/dev/null || printf ''; }

tool=$(field '.tool_name')

refuse_identifier_grep() {
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
}

refuse_shell_search() {
  cat >&2 <<EOF
\`${first}\` over source files is text search, and text search cannot tell a definition
from a call site, an import, or a same-named symbol in another module.

  Definition ............ serena: find_symbol
  Usages ................ serena: find_referencing_symbols
  File structure ........ serena: get_symbols_overview
  Structural pattern .... ast-grep run -p '<pattern>' -l rust|ts|tsx|go

Shell search is still right for files with no AST node — scope it to docs/, .github/,
.pm-playbook/, examples/, a *.md / *.toml / *.json / *.yml glob, or an \`-t md\` type filter —
and for filtering command output through a pipe (\`cargo test | grep 'test result'\`).

Refused command segment: ${seg}
EOF
  exit 2
}

refuse_shell_read() {
  cat >&2 <<EOF
\`${first}\` on a source file reads code as text. Read it through the language server or the
dedicated tool instead:

  Whole-file structure .. serena: get_symbols_overview
  One symbol's body ..... serena: find_symbol with include_body
  A file or range ....... the Read tool

Scratch files under /tmp or scratchpad/ pass through.

Refused command segment: ${seg}
EOF
  exit 2
}

refuse_shell_edit() {
  cat >&2 <<EOF
\`${first}\` changes a source file as text, and a line-oriented edit cannot see the symbol it
is inside. Edit through the language server or the dedicated tool instead:

  Replace a symbol ...... serena: replace_symbol_body
  Insert beside one ..... serena: insert_after_symbol / insert_before_symbol
  Exact-string edit ..... serena: replace_content, or the Edit tool
  New file .............. the Write tool

Scratch files under /tmp or scratchpad/ pass through.

Refused command segment: ${seg}
EOF
  exit 2
}

if [ "$tool" = "Bash" ]; then
  cmd=$(field '.tool_input.command')
  [ -n "$cmd" ] || exit 0

  code_file='\.(rs|ts|tsx|js|mjs|cjs|go)([^A-Za-z0-9_]|$)'
  scratch='(^|[ =/])(/tmp|/private/tmp|scratchpad|target|node_modules)/'
  noncode_scope='\.(md|mdx|json|jsonc|ya?ml|toml|sql|css|scss|txt|lock|proto|sh|log|snap|env)([^A-Za-z0-9_]|$)|(^|[ =/])(docs|\.github|\.pm-playbook|examples|migrations|vendor|scratchpad|\.serena|\.claude)/|/tmp/|~/\.|\$HOME/\.|(^| )(-t|--type)[ =]?(md|toml|json|yaml|txt|sh)( |$)'
  redirect_into_code='>>?[[:space:]]*[^[:space:]>]*\.(rs|ts|tsx|js|mjs|cjs|go)([^A-Za-z0-9_]|$)'

  segments=$(awk '{
    gsub(/[|][|]/, "\n"); gsub(/&&/, "\n"); gsub(/;/, "\n")
    gsub(/[$][(]/, "\n"); gsub(/[(]/, "\n"); gsub(/[|]/, "\n@PIPE@ ")
    print }' <<<"$cmd")

  while IFS= read -r seg; do
    piped=0
    case "$seg" in @PIPE@*) piped=1; seg=${seg#@PIPE@ } ;; esac
    [ -n "${seg// /}" ] || continue

    first=$(awk '{ for (i = 1; i <= NF; i++) {
      if ($i ~ /^[A-Za-z_][A-Za-z0-9_]*=/) continue
      if ($i ~ /^(sudo|time|command|env|nice|exec|builtin|xargs|do|then|else|if|while|until)$/) continue
      if ($i == "{" || $i == "!") continue
      print $i; exit } }' <<<"$seg")
    first=${first##*/}
    [[ "$seg" =~ (^|[[:space:]])git[[:space:]]+grep([[:space:]]|$) ]] && first=grep
    [[ "$seg" =~ (^|[[:space:]])xargs[[:space:]] ]] && piped=0

    touches_code=0
    if grep -Eq "$code_file" <<<"$seg" && ! grep -Eq "$scratch" <<<"$seg"; then touches_code=1; fi

    case "$first" in
      grep|egrep|fgrep|rg|ag|ack)
        [ "$piped" = 1 ] && continue
        grep -Eq "$noncode_scope" <<<"$seg" && continue
        refuse_shell_search
        ;;
      sed|perl)
        if [ "$touches_code" = 1 ] && grep -Eq '(^|[[:space:]])-(i|pi|0pi|pie|pI)' <<<"$seg"; then refuse_shell_edit; fi
        if [ "$first" = sed ] && [ "$touches_code" = 1 ]; then refuse_shell_read; fi
        ;;
      tee)
        [ "$touches_code" = 1 ] && refuse_shell_edit
        ;;
      cat|head|tail|less|more|bat|nl|tac)
        if [ "$touches_code" = 1 ]; then
          if grep -Eq "$redirect_into_code" <<<"$seg"; then refuse_shell_edit; else refuse_shell_read; fi
        fi
        ;;
    esac

    if [ "$touches_code" = 1 ] && grep -Eq "$redirect_into_code" <<<"$seg"; then refuse_shell_edit; fi
  done <<<"$segments"

  exit 0
fi

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

refuse_identifier_grep
