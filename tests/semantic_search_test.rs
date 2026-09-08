use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

fn hook_path() -> PathBuf {
    repo_root().join(".claude/hooks/no-grep.sh")
}

fn run_hook(payload: &str) -> i32 {
    let hook = hook_path();
    let mut child = Command::new(&hook)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("cannot spawn {}: {e}", hook.display()));
    child
        .stdin
        .take()
        .expect("hook stdin was not piped")
        .write_all(payload.as_bytes())
        .ok();
    let status = child
        .wait()
        .unwrap_or_else(|e| panic!("hook did not exit: {e}"));
    status
        .code()
        .unwrap_or_else(|| panic!("hook was killed by a signal on payload: {payload}"))
}

const BLOCK: i32 = 2;
const ALLOW: i32 = 0;

#[test]
fn jq_is_present_or_the_hook_stops_enforcing_without_saying_so() {
    let found = Command::new("jq").arg("--version").output();
    assert!(
        found.is_ok(),
        "jq is not on PATH. The hook shells out to jq to read the PreToolUse payload and \
         swallows the failure, so without jq it returns 0 for every pattern and identifier \
         search is silently unguarded rather than loudly broken. Every assertion below would \
         still pass except the blocking ones."
    );
}

#[test]
fn the_hook_is_wired_from_settings_and_is_executable() {
    let settings = read(".claude/settings.json");
    let parsed: serde_json::Value =
        serde_json::from_str(&settings).expect(".claude/settings.json is not valid JSON");

    let entries = parsed["hooks"]["PreToolUse"]
        .as_array()
        .expect("settings.json has no hooks.PreToolUse array");
    let grep_entry = entries
        .iter()
        .find(|e| e["matcher"] == "Grep")
        .expect("no PreToolUse entry matches the Grep tool, so the hook never runs");
    let command = grep_entry["hooks"][0]["command"]
        .as_str()
        .expect("the Grep PreToolUse entry has no command");
    assert!(
        command.ends_with(".claude/hooks/no-grep.sh"),
        "settings.json points PreToolUse at {command}, which is not the committed hook"
    );

    assert!(
        hook_path().exists(),
        "settings.json references {} but it does not exist",
        hook_path().display()
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(hook_path()).unwrap().permissions().mode();
        assert!(
            mode & 0o111 != 0,
            "{} is not executable (mode {mode:o}); the hook fails to spawn and every Grep \
             proceeds unchecked",
            hook_path().display()
        );
    }
}

#[test]
fn an_identifier_shaped_pattern_is_refused() {
    for pattern in [
        "CommitSequencer",
        "find_symbol",
        "forgedb_storage::FixedColumn",
        "Model.identity_field",
        "_leading_underscore",
    ] {
        let payload =
            format!(r#"{{"tool_name":"Grep","tool_input":{{"pattern":"{pattern}","glob":"*.rs"}}}}"#);
        assert_eq!(
            run_hook(&payload),
            BLOCK,
            "{pattern} is identifier-shaped over Rust and must route to serena/ast-grep. \
             Text search cannot tell a definition from a call site or a same-named symbol \
             in another module."
        );
    }
}

#[test]
fn text_search_for_things_with_no_ast_node_still_works() {
    let cases = [
        r#"{"tool_name":"Grep","tool_input":{"pattern":"\"payment_failed\""}}"#,
        r#"{"tool_name":"Grep","tool_input":{"pattern":"DO NOT EDIT"}}"#,
        r#"{"tool_name":"Grep","tool_input":{"pattern":"test result: ok\\. (\\d+) passed"}}"#,
        r#"{"tool_name":"Grep","tool_input":{"pattern":"version","glob":"*.toml"}}"#,
        r#"{"tool_name":"Grep","tool_input":{"pattern":"Serena","glob":"*.md"}}"#,
        r#"{"tool_name":"Grep","tool_input":{"pattern":"milestone","path":"/x/.pm-playbook/backlog"}}"#,
    ];
    for payload in cases {
        assert_eq!(
            run_hook(payload),
            ALLOW,
            "string literals, error copy, config keys and non-code globs have no reachable AST \
             node — 934 quote! bodies in crates/codegen alone — so refusing them buys friction \
             with no cover. Payload: {payload}"
        );
    }
}

#[test]
fn a_malformed_payload_cannot_block_every_search() {
    for payload in [
        "",
        "this is not json at all {{{",
        r#"{"tool_name":"Grep"}"#,
        r#"{"tool_name":"Grep","tool_input":{"pattern":null}}"#,
        "{}",
        "[1,2,3]",
    ] {
        assert_eq!(
            run_hook(payload),
            ALLOW,
            "a hook that errors or hangs on an unexpected payload blocks every search in the \
             session, which is worse than not enforcing at all. Payload: {payload:?}"
        );
    }
}

#[test]
fn the_serena_config_is_complete_so_serena_does_not_rewrite_it_on_every_start() {
    let yml = read(".serena/project.yml");

    assert!(
        !yml.lines().any(|l| l.trim_start().starts_with("languages:")),
        ".serena/project.yml still uses the legacy `languages:` key. Serena renames it to \
         `language_servers:` and re-saves the file, which dirties the working tree on every \
         session start."
    );

    for key in [
        "language_servers:",
        "ignored_paths:",
        "ls_specific_settings:",
        "language_backend:",
        "ls_workspace_folders:",
        "encoding:",
        "project_name:",
    ] {
        assert!(
            yml.lines().any(|l| l.starts_with(key)),
            "`{key}` is missing from .serena/project.yml. Serena re-saves the file whenever a \
             top-level key is absent, so a partial config is rewritten on every start and never \
             stays committed."
        );
    }

    assert!(
        yml.contains("allFeatures: true"),
        "cargo.allFeatures is not set; code behind a non-default feature such as forgedb-auth's \
         jwks-http resolves as though the feature were off"
    );
    assert!(
        yml.contains("scratchpad/"),
        "scratchpad/ is gitignored but present on disk with ~50 throwaway crates carrying real \
         symbol names such as Database and Storage. Serena indexes the filesystem, not the git \
         index, so an unexcluded scratchpad makes ambiguity look like a real answer."
    );
}

#[test]
fn the_trust_requirement_is_documented_because_no_committed_file_can_carry_it() {
    let dev = read("docs/DEVELOPMENT.md");
    for needle in [
        "trusted_project_path_patterns",
        "serena_config.yml",
        "is not trusted, ignoring LS-specific settings",
    ] {
        assert!(
            dev.contains(needle),
            "docs/DEVELOPMENT.md does not mention `{needle}`. ls_specific_settings — where \
             allFeatures lives — is discarded unless the checkout is trusted in a machine-global \
             untracked file, and Serena reports that only in its log. Undocumented, every fresh \
             clone silently loses feature-gated resolution."
        );
    }

    let claude_md = read("CLAUDE.md");
    assert!(
        claude_md.contains("trusted_project_path_patterns"),
        "CLAUDE.md's Code search section must state the trust requirement; it is the file an \
         agent reads before deciding whether an empty result means absence"
    );
}

#[test]
fn serena_working_files_are_ignored_and_the_committed_ones_are_not() {
    let root = read(".gitignore");
    let serena = read(".serena/.gitignore");
    let combined = format!("{root}\n{serena}");

    for needle in ["cache", "project.local.yml", "memories"] {
        assert!(
            combined.contains(needle),
            "`{needle}` is not ignored. Serena writes it under .serena/ on every run, so it \
             shows up as untracked noise in every diff."
        );
    }

    assert!(
        !root.contains("/.serena/project.yml"),
        ".serena/project.yml must stay tracked — it carries language_servers, ignored_paths and \
         allFeatures, and a clone without it has no Serena configuration at all"
    );
}

#[test]
fn the_mcp_server_is_declared_and_pinned_to_the_project() {
    let mcp = read(".mcp.json");
    let parsed: serde_json::Value =
        serde_json::from_str(&mcp).expect(".mcp.json is not valid JSON");
    let serena = &parsed["mcpServers"]["serena"];
    assert!(
        !serena.is_null(),
        ".mcp.json declares no `serena` server, so no session starts one and every symbol lookup \
         silently falls back to text search"
    );
    let args = serena["args"]
        .as_array()
        .expect("the serena server entry has no args")
        .iter()
        .filter_map(|a| a.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        args.contains("--context claude-code"),
        "serena must run with --context claude-code so it drops the tools that duplicate the \
         built-in file and shell operations; args were: {args}"
    );
    assert!(
        args.contains("--project"),
        "serena must be pinned to this project or it indexes whatever directory it starts in; \
         args were: {args}"
    );
}
