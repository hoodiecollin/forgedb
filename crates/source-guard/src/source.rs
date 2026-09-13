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

    pub fn walk(dir: impl AsRef<Path>, skip_dirs: &[&str]) -> Vec<Self> {
        fn collect(dir: &Path, skip: &[&str], out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !skip.contains(&name) {
                        collect(&path, skip, out);
                    }
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let mut paths = Vec::new();
        collect(dir.as_ref(), skip_dirs, &mut paths);
        paths.sort();
        paths.into_iter().map(Self::repo_file).collect()
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
