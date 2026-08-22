//! Scoping queries — locate a named item and assert *within* it.
//!
//! This is the dominant need, and the parent issue understates it. Classifying all 77
//! `.find(` probes in `codegen_snapshots.rs`:
//!
//! | Class | Count |
//! |---|---|
//! | window / offset-binding-for-a-window | **~42** |
//! | ordering (two offsets compared) | ~5 |
//! | existence (`find` used as `contains`) | 1 |
//! | other | 29 |
//!
//! So the headline `stmt_index_of` API addresses the *smaller* class. Scoping is what the
//! suite actually does with substring search, and scoping is where a miss is most
//! dangerous.
//!
//! # The rule
//!
//! **A scope that cannot be located is an error.** Never an empty scope, never a wider one.
//!
//! The idiom being replaced degrades in the worst possible direction. `&code[code.find(x)
//! .unwrap_or(0)..]` widens to the whole file when `x` rots, and even on a hit it runs to
//! EOF. Measured on one real schema: the "insert body" covered 237 KB of a 261 KB file and
//! contained eight `.write(&forgedb_wal::WalEntry` calls belonging to seven *other*
//! methods, so the guard passed with the call it names deleted outright.
//!
//! A vacuous pass at least stops claiming anything. A widened scope keeps claiming the same
//! thing about a bigger haystack — so it gets *easier* to satisfy exactly as it becomes
//! meaningless. That asymmetry is why every query here returns `Result` and every failure
//! lists what *is* present.

use std::fmt;

use crate::RustSource;

/// A scope that could not be located.
///
/// An error type rather than an `Option`, deliberately: an `Option` invites `unwrap_or`,
/// and `unwrap_or` is the exact defect this crate exists to delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeError {
    what: String,
    origin: String,
    available: Vec<String>,
}

impl fmt::Display for ScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} not found in {}", self.what, self.origin)?;
        if self.available.is_empty() {
            write!(f, " — nothing of that kind is present at all")
        } else {
            // Name what IS there. A guard that rots because of a rename is far cheaper to
            // repair when the failure hands you the new spelling instead of just "false".
            write!(f, " — available: {}", self.available.join(", "))
        }
    }
}

impl std::error::Error for ScopeError {}

/// A located function, and the things worth asserting about its body.
///
/// `Debug` is hand-written rather than derived: the `syn` nodes inside would dump an entire
/// syntax tree into any `expect_err` failure message, which buries the actual assertion.
pub struct FnScope<'a> {
    pub(crate) item: &'a syn::ItemFn,
    pub(crate) sig: &'a syn::Signature,
    pub(crate) block: &'a syn::Block,
    origin: String,
}

/// A located method inside an `impl`. See [`FnScope`] on why `Debug` is hand-written.
pub struct MethodScope<'a> {
    pub(crate) sig: &'a syn::Signature,
    pub(crate) block: &'a syn::Block,
    origin: String,
}

macro_rules! body_queries {
    ($t:ty) => {
        impl<'a> $t {
            /// The body block. `block.stmts` **is** statement order — that is the whole
            /// point of comparing indices into it rather than byte offsets into a blob.
            pub fn block(&self) -> &'a syn::Block {
                self.block
            }

            /// The signature.
            pub fn sig(&self) -> &'a syn::Signature {
                self.sig
            }

            /// How many times this body calls a function or method named `name`.
            ///
            /// Counts by AST node, so it does not match the name in a comment, in a string
            /// literal, or as part of a longer identifier — the three things a substring
            /// cannot separate. Bare paths (`foo()`), associated paths (`T::foo()`) and
            /// method calls (`x.foo()`) all count.
            pub fn call_count(&self, name: &str) -> usize {
                let mut v = CallCounter {
                    name,
                    count: 0,
                };
                syn::visit::visit_block(&mut v, self.block);
                v.count
            }

            /// Whether this body calls `name` at all.
            pub fn calls(&self, name: &str) -> bool {
                self.call_count(name) > 0
            }

            /// How many times this body *reads* a field named `name` (`expr.name`).
            ///
            /// Distinct from the field's declaration and from a struct-literal
            /// initializer — three different `syn` nodes that one substring conflates.
            pub fn field_read_count(&self, name: &str) -> usize {
                let mut v = FieldReadCounter { name, count: 0 };
                syn::visit::visit_block(&mut v, self.block);
                v.count
            }

            /// Index of the first statement satisfying `pred`, for ordering assertions.
            pub fn stmt_index_of(&self, pred: impl Fn(&syn::Stmt) -> bool) -> Option<usize> {
                self.block.stmts.iter().position(pred)
            }

            /// The declared type of the parameter named `name`, rendered as compact source
            /// text (`Option<f64>`, not `Option < f64 >`).
            ///
            /// Replaces the two-spelling dance that byte matching forces —
            /// `contains("min : Option < f64 >") || contains("min: Option<f64>")` — which
            /// exists only because prettyplease's spacing is not stable across contexts.
            /// The type is one thing; it should be asked for once.
            ///
            /// `None` when there is no such parameter, so a caller can distinguish "absent"
            /// from "present with a different type" instead of collapsing both to false.
            pub fn param_type(&self, name: &str) -> Option<String> {
                self.sig.inputs.iter().find_map(|arg| match arg {
                    syn::FnArg::Typed(pt) => match &*pt.pat {
                        syn::Pat::Ident(id) if id.ident == name => {
                            Some(crate::scope::render_type(&pt.ty))
                        }
                        _ => None,
                    },
                    syn::FnArg::Receiver(_) => None,
                })
            }

            /// Every parameter name, in declaration order — for a failure message that can
            /// say what IS there.
            pub fn param_names(&self) -> Vec<String> {
                self.sig
                    .inputs
                    .iter()
                    .filter_map(|arg| match arg {
                        syn::FnArg::Typed(pt) => match &*pt.pat {
                            syn::Pat::Ident(id) => Some(id.ident.to_string()),
                            _ => None,
                        },
                        syn::FnArg::Receiver(_) => Some("self".to_string()),
                    })
                    .collect()
            }

            /// The body rendered as a token string, whitespace-normalized by the tokenizer.
            ///
            /// **The escape hatch, scoped.** It exists because some assertions are about
            /// emitted *shape* (a match arm, a local struct declaration) that the query
            /// surface does not yet express, and forcing those through the AST now would
            /// mean either a worse assertion or a much larger change.
            ///
            /// The important difference from the byte window it replaces: this text is
            /// bounded by the AST to **this body**. It cannot widen. A rotted anchor fails
            /// at the lookup rather than silently handing back the rest of the file.
            ///
            /// Still a substring match, so it still cannot tell a call from a comment.
            /// Grep `body_text_because` to find every assertion still on that footing —
            /// the list should shrink, and must never grow silently.
            pub fn body_text_because(&self, why: &str) -> String {
                debug_assert!(
                    !why.trim().is_empty(),
                    "body_text_because needs a real reason, not an empty string"
                );
                use quote::ToTokens;
                self.block.to_token_stream().to_string()
            }

            /// Where this scope came from, for messages.
            pub fn origin(&self) -> &str {
                &self.origin
            }
        }
    };
}

body_queries!(FnScope<'a>);
body_queries!(MethodScope<'a>);

impl fmt::Debug for FnScope<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FnScope({}, {} stmts)", self.origin, self.block.stmts.len())
    }
}

impl fmt::Debug for MethodScope<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MethodScope({}, {} stmts)", self.origin, self.block.stmts.len())
    }
}

impl<'a> FnScope<'a> {
    /// The whole item, when a query needs attributes or visibility.
    pub fn item(&self) -> &'a syn::ItemFn {
        self.item
    }
}

// ---------------------------------------------------------------------------
// visitors
// ---------------------------------------------------------------------------

struct CallCounter<'n> {
    name: &'n str,
    count: usize,
}

impl<'ast, 'n> syn::visit::Visit<'ast> for CallCounter<'n> {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
            // Last segment: matches both `foo()` and `Type::foo()`.
            if p.path
                .segments
                .last()
                .is_some_and(|s| s.ident == self.name)
            {
                self.count += 1;
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == self.name {
            self.count += 1;
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

struct FieldReadCounter<'n> {
    name: &'n str,
    count: usize,
}

impl<'ast, 'n> syn::visit::Visit<'ast> for FieldReadCounter<'n> {
    fn visit_expr_field(&mut self, node: &'ast syn::ExprField) {
        if let syn::Member::Named(id) = &node.member
            && id == self.name
        {
            self.count += 1;
        }
        syn::visit::visit_expr_field(self, node);
    }
}

// ---------------------------------------------------------------------------
// lookups
// ---------------------------------------------------------------------------

impl RustSource {
    pub(crate) fn scope_error(&self, what: impl Into<String>, available: Vec<String>) -> ScopeError {
        ScopeError {
            what: what.into(),
            origin: self.origin().to_string(),
            available,
        }
    }

    /// A free function named `name`.
    pub fn fn_named(&self, name: &str) -> Result<FnScope<'_>, ScopeError> {
        let mut found = self.ast().items.iter().filter_map(|it| match it {
            syn::Item::Fn(f) if f.sig.ident == name => Some(f),
            _ => None,
        });

        match (found.next(), found.next()) {
            (Some(f), None) => Ok(FnScope {
                item: f,
                sig: &f.sig,
                block: &f.block,
                origin: format!("{}::fn {name}", self.origin()),
            }),
            (Some(_), Some(_)) => Err(self.scope_error(
                format!("free fn `{name}` is AMBIGUOUS (declared more than once)"),
                vec![],
            )),
            (None, _) => Err(self.scope_error(
                format!("free fn `{name}`"),
                self.ast()
                    .items
                    .iter()
                    .filter_map(|it| match it {
                        syn::Item::Fn(f) => Some(f.sig.ident.to_string()),
                        _ => None,
                    })
                    .collect(),
            )),
        }
    }

    /// A method named `name`, which must be declared **exactly once** across all `impl`
    /// blocks in this file.
    ///
    /// Ambiguity is an error rather than "first wins". Generated code repeats method names
    /// across per-model impls — `insert` exists once per model — and silently taking the
    /// first is how a guard ends up asserting about a model it was not written for. The
    /// error names every owner so the caller can pick with [`RustSource::method_in`].
    pub fn method_named(&self, name: &str) -> Result<MethodScope<'_>, ScopeError> {
        let hits = self.methods_matching(|imp, m| {
            let _ = imp;
            m.sig.ident == name
        });

        match hits.len() {
            1 => {
                let (owner, m) = &hits[0];
                Ok(MethodScope {
                    sig: &m.sig,
                    block: &m.block,
                    origin: format!("{}::{owner}::{name}", self.origin()),
                })
            }
            0 => Err(self.scope_error(format!("method `{name}`"), self.method_names())),
            _ => Err(self.scope_error(
                format!(
                    "method `{name}` is AMBIGUOUS — declared {} times; use `method_in`",
                    hits.len()
                ),
                hits.iter().map(|(o, _)| o.clone()).collect(),
            )),
        }
    }

    /// **Every** method named `name`, across all `impl` blocks, with its owner.
    ///
    /// This is usually what a guard over generated code actually means. Generated
    /// `database.rs` emits one `insert` / `__stage_append` / `__with_scan` *per model*, so
    /// `code.find("fn __stage_append")` silently asserted about whichever storage was
    /// emitted first and never looked at the rest — a two-model schema got a one-model
    /// guard, and nothing said so.
    ///
    /// Prefer this to picking one owner when the property is supposed to hold for all of
    /// them. Returns an error when there are none, so a rotted name still fails loudly.
    pub fn methods_named(&self, name: &str) -> Result<Vec<(String, MethodScope<'_>)>, ScopeError> {
        let hits = self.methods_matching(|_, m| m.sig.ident == name);
        if hits.is_empty() {
            return Err(self.scope_error(format!("method `{name}`"), self.method_names()));
        }
        Ok(hits
            .into_iter()
            .map(|(owner, m)| {
                let scope = MethodScope {
                    sig: &m.sig,
                    block: &m.block,
                    origin: format!("{}::{owner}::{name}", self.origin()),
                };
                (owner, scope)
            })
            .collect())
    }

    /// A method named `name` on the impl whose self type renders as `ty`.
    pub fn method_in(&self, ty: &str, name: &str) -> Result<MethodScope<'_>, ScopeError> {
        let hits = self.methods_matching(|imp, m| imp == ty && m.sig.ident == name);

        match hits.len() {
            1 => {
                let (owner, m) = &hits[0];
                Ok(MethodScope {
                    sig: &m.sig,
                    block: &m.block,
                    origin: format!("{}::{owner}::{name}", self.origin()),
                })
            }
            0 => Err(self.scope_error(
                format!("method `{ty}::{name}`"),
                self.method_names(),
            )),
            _ => Err(self.scope_error(
                format!("method `{ty}::{name}` is AMBIGUOUS ({} matches)", hits.len()),
                vec![],
            )),
        }
    }

    /// A struct named `name`.
    pub fn struct_named(&self, name: &str) -> Result<&syn::ItemStruct, ScopeError> {
        let mut found = self.ast().items.iter().filter_map(|it| match it {
            syn::Item::Struct(s) if s.ident == name => Some(s),
            _ => None,
        });

        match (found.next(), found.next()) {
            (Some(s), None) => Ok(s),
            (Some(_), Some(_)) => Err(self.scope_error(
                format!("struct `{name}` is AMBIGUOUS (declared more than once)"),
                vec![],
            )),
            (None, _) => Err(self.scope_error(
                format!("struct `{name}`"),
                self.ast()
                    .items
                    .iter()
                    .filter_map(|it| match it {
                        syn::Item::Struct(s) => Some(s.ident.to_string()),
                        _ => None,
                    })
                    .collect(),
            )),
        }
    }

    /// The initializer expression of the struct-literal field named `name`, anywhere in
    /// this file, rendered as compact source text.
    ///
    /// Replaces "find `<name>: ` in a flattened blob and take the next N characters",
    /// where N was a guess. An expression has exact bounds; a character budget does not,
    /// and when the terminator needle rots the budget silently becomes the answer.
    ///
    /// Returns **every distinct** initializer, deduped. Generated code initializes the
    /// same field in several places — an open path, a create path, a scan-buffer holder —
    /// so there is rarely exactly one, and taking the first (as the byte form did) silently
    /// answers about whichever the emitter happened to put first. The caller decides which
    /// shape it means, and can assert how many there are.
    ///
    /// Absent is still an error: a rotted field name must fail, not return an empty list
    /// that every `all()` check passes vacuously.
    pub fn struct_literal_field_inits(&self, name: &str) -> Result<Vec<String>, ScopeError> {
        use quote::ToTokens;

        struct Collect<'n> {
            name: &'n str,
            hits: Vec<String>,
            seen: Vec<String>,
        }
        impl<'ast, 'n> syn::visit::Visit<'ast> for Collect<'n> {
            fn visit_field_value(&mut self, node: &'ast syn::FieldValue) {
                if let syn::Member::Named(id) = &node.member {
                    self.seen.push(id.to_string());
                    if id == self.name {
                        self.hits
                            .push(node.expr.to_token_stream().to_string().replace(" :: ", "::"));
                    }
                }
                syn::visit::visit_field_value(self, node);
            }
        }

        let mut c = Collect {
            name,
            hits: Vec::new(),
            seen: Vec::new(),
        };
        syn::visit::visit_file(&mut c, self.ast());

        if c.hits.is_empty() {
            c.seen.sort();
            c.seen.dedup();
            return Err(self.scope_error(format!("struct-literal field `{name}`"), c.seen));
        }
        c.hits.sort();
        c.hits.dedup();
        Ok(c.hits)
    }

    /// The declared type of `field` on `struct_name`, rendered as source text.
    ///
    /// Replaces `flat.contains("pubid:String")`, which matches `pub id: String` in *any*
    /// struct in the file and also matches `pub id: Stringify`.
    pub fn field_type(&self, struct_name: &str, field: &str) -> Result<String, ScopeError> {
        let s = self.struct_named(struct_name)?;
        let names: Vec<String> = s
            .fields
            .iter()
            .filter_map(|f| f.ident.as_ref().map(|i| i.to_string()))
            .collect();

        s.fields
            .iter()
            .find(|f| f.ident.as_ref().is_some_and(|i| i == field))
            .map(|f| render_type(&f.ty))
            .ok_or_else(|| self.scope_error(format!("field `{struct_name}.{field}`"), names))
    }

    fn methods_matching(
        &self,
        pred: impl Fn(&str, &syn::ImplItemFn) -> bool,
    ) -> Vec<(String, &syn::ImplItemFn)> {
        let mut out = Vec::new();
        for item in &self.ast().items {
            let syn::Item::Impl(imp) = item else { continue };
            let owner = render_type(&imp.self_ty);
            for ii in &imp.items {
                if let syn::ImplItem::Fn(m) = ii
                    && pred(&owner, m)
                {
                    out.push((owner.clone(), m));
                }
            }
        }
        out
    }

    fn method_names(&self) -> Vec<String> {
        self.methods_matching(|_, _| true)
            .into_iter()
            .map(|(o, m)| format!("{o}::{}", m.sig.ident))
            .collect()
    }
}

/// Render a type as compact source text (`Option < Vec < usize > >` → `Option<Vec<usize>>`).
pub(crate) fn render_type(ty: &syn::Type) -> String {
    use quote::ToTokens;
    ty.to_token_stream()
        .to_string()
        .replace(" :: ", "::")
        .replace(" < ", "<")
        .replace(" > ", ">")
        .replace(" >", ">")
        .replace("< ", "<")
        .replace(" ,", ",")
        .replace(" ;", ";")
}
