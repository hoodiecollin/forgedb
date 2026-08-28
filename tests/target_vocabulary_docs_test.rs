use forgedb::targets::{DEPRECATED, VOCABULARY};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn site_page() -> String {
    let path = repo_root().join("apps/website/content/docs/config/generate.mdx");
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "the target vocabulary's user-facing documentation is missing at {}: {e}. \
             It is the ONLY place these values are documented — ForgeDB's source carries \
             no doc comments (#488), so this page is the whole record",
            path.display()
        )
    })
}

#[test]
fn every_vocabulary_row_is_documented_where_users_read_it() {
    let page = site_page();

    assert!(
        page.len() > 500,
        "the generate-config page is too short to be documenting anything: {} bytes",
        page.len()
    );

    for row in VOCABULARY {
        assert!(
            page.contains(&format!("`{}`", row.config)),
            "`{}` is a legal `[generate].targets` value but appears nowhere in \
             apps/website/content/docs/config/generate.mdx — a user cannot discover it",
            row.config
        );
    }
}

#[test]
fn every_deprecated_spelling_is_documented_where_users_read_it() {
    let page = site_page();

    for (old, replacement) in DEPRECATED {
        assert!(
            page.contains(&format!("`{old}`")),
            "the deprecated spelling `{old}` still parses (and warns, pointing at \
             `{replacement}`) but is undocumented on the site, so a user who hits the \
             warning has nowhere to read what it means"
        );
    }
}
