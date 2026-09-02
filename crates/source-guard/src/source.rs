use std::path::Path;
use std::rc::Rc;

use crate::cache::cached_parse;

pub struct RustSource {
    file: Rc<syn::File>,
    text: String,
    origin: String,
}

impl RustSource {
    pub fn generated(origin: impl Into<String>, code: impl Into<String>) -> Self {
        let text = code.into();
        Self {
            file: cached_parse(&text),
            text,
            origin: origin.into(),
        }
    }

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

    pub fn ast(&self) -> &syn::File {
        &self.file
    }

    pub fn raw_text_because(&self, why: &str) -> &str {
        debug_assert!(
            !why.trim().is_empty(),
            "raw_text_because needs a real reason, not an empty string"
        );
        &self.text
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

}
