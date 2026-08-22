//! The parsed-source wrapper.

use std::path::Path;
use std::rc::Rc;

use crate::cache::cached_parse;

/// Rust source plus its parsed tree.
///
/// Two constructors, because the two subjects carry different parse guarantees and the
/// distinction is worth keeping in the type's API rather than in a comment:
///
/// * [`RustSource::generated`] — output of one of this workspace's generators. It is
///   guaranteed parseable: every Rust generator round-trips through `syn` and hard-fails
///   before returning, so a test-side parse adds no new risk.
/// * [`RustSource::repo_file`] — a handwritten file in the tree. It carries no such
///   promise from generation; it parses because it is real Rust that rustc compiles.
pub struct RustSource {
    file: Rc<syn::File>,
    text: String,
    origin: String,
}

impl RustSource {
    /// Wrap generated code. `origin` is used only in failure messages.
    pub fn generated(origin: impl Into<String>, code: impl Into<String>) -> Self {
        let text = code.into();
        Self {
            file: cached_parse(&text),
            text,
            origin: origin.into(),
        }
    }

    /// Read and wrap a handwritten file from the repo.
    ///
    /// # Panics
    ///
    /// If the file cannot be read, or does not parse. Both are hard failures on purpose —
    /// a guard that skips an unreadable subject reports green because it never evaluated.
    pub fn repo_file(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("source-guard: cannot read {}: {e}", path.display()));
        Self {
            file: cached_parse(&text),
            text,
            origin: path.display().to_string(),
        }
    }

    /// The parsed tree.
    pub fn ast(&self) -> &syn::File {
        &self.file
    }

    /// The original text.
    ///
    /// This is the **escape hatch**, and it is deliberately not called `as_str`. Reaching
    /// for it puts you back on substring matching with all the failure modes above, so it
    /// takes a `why` that is recorded at the call site. Grep for `raw_text_because` to find
    /// every place the AST was not enough — that list should shrink, and should never grow
    /// silently.
    pub fn raw_text_because(&self, why: &str) -> &str {
        debug_assert!(
            !why.trim().is_empty(),
            "raw_text_because needs a real reason, not an empty string"
        );
        &self.text
    }

    /// Where this source came from, for messages.
    pub fn origin(&self) -> &str {
        &self.origin
    }

}
