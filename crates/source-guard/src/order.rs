//! Ordering queries — "does A happen before B", answered structurally.
//!
//! # Why not byte offsets
//!
//! The idiom being replaced compares `str::find` results into a whitespace-flattened blob.
//! It fails in two directions that matter:
//!
//! * **A rot in any anchor silently changes the claim.** `prettyplease` breaks lines by
//!   length, so the same emitted call renders `return db .post .__with_page(` in one schema
//!   and unbroken in another. An anchor that stops matching either panics (if it uses
//!   `expect`) or, far worse, quietly compares against a fallback.
//! * **It cannot tell code from prose.** A needle inside a comment or a string literal is
//!   an offset like any other, so a doc comment mentioning the call reorders the claim.
//!
//! # Why pre-order position rather than `Block.stmts` index
//!
//! A statement index only orders *siblings*. The real guards compare things at different
//! nesting depths — a `let` at the top of a function against a call inside an `if` branch
//! against a `return` after it. `stmts.iter().position(…)` cannot see into the branch, so
//! it would answer `None` for exactly the anchors that matter.
//!
//! A **pre-order traversal ordinal** does order them, and it orders them the way a reader
//! means: the tick advances on every statement and every expression, in source order,
//! through nesting. Comparing two ordinals answers "which is reached first in the source"
//! and nothing else — no bytes, no formatting, no prose.
//!
//! # The acceptance case
//!
//! Commit `0c9b802` — *"anchor the ordering guard on the pushdown call, not the binding"*.
//! The original #281 guard located the scan path by `let __sel: Option<Vec<usize>> =`.
//! Resolving the index probe into an earlier binding and rebinding it at the old site
//! (`let __sel = __sel_early;`) left the *name* exactly where it was, so a name-anchored
//! assertion stayed **green** while the probe had moved onto the fast path — precisely the
//! waste the ordering exists to prevent.
//!
//! That is why [`Marker::Call`] exists and why guards should prefer it: a call is the
//! **work**, a binding is only its label. Mutating a label is cheap and invisible; moving
//! the work is what the guard is about.

use crate::scope::{FnScope, MethodScope};

/// A thing whose position in a body can be asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker<'a> {
    /// A call to a function or method of this name — `foo()`, `T::foo()`, or `x.foo()`.
    ///
    /// **Prefer this to [`Marker::LetBinding`].** A call is the work; a binding is its
    /// label, and a label can be moved without moving anything real. See `0c9b802`.
    Call(&'a str),
    /// A `let` binding of this name. Use only when there is no call to anchor on.
    LetBinding(&'a str),
}

struct Walker<'m> {
    markers: &'m [Marker<'m>],
    found: Vec<Option<usize>>,
    tick: usize,
}

impl<'m> Walker<'m> {
    fn hit(&mut self, m: Marker<'_>) {
        for (i, want) in self.markers.iter().enumerate() {
            if *want == m && self.found[i].is_none() {
                self.found[i] = Some(self.tick);
            }
        }
    }
}

impl<'ast, 'm> syn::visit::Visit<'ast> for Walker<'m> {
    fn visit_stmt(&mut self, node: &'ast syn::Stmt) {
        self.tick += 1;
        if let syn::Stmt::Local(local) = node
            && let syn::Pat::Ident(id) = &local.pat
        {
            let name = id.ident.to_string();
            self.hit(Marker::LetBinding(&name));
        }
        // A `let` with a type annotation parses as `Pat::Type` wrapping `Pat::Ident`, which
        // is how EVERY annotated binding in the generated code looks
        // (`let __sel: Option<Vec<usize>> = …`). Missing this arm would answer `None` for
        // the majority of real bindings — the silent-hole failure this crate exists to
        // delete, reproduced inside the crate itself.
        if let syn::Stmt::Local(local) = node
            && let syn::Pat::Type(pt) = &local.pat
            && let syn::Pat::Ident(id) = &*pt.pat
        {
            let name = id.ident.to_string();
            self.hit(Marker::LetBinding(&name));
        }
        syn::visit::visit_stmt(self, node);
    }

    fn visit_expr(&mut self, node: &'ast syn::Expr) {
        self.tick += 1;
        match node {
            syn::Expr::Call(c) => {
                if let syn::Expr::Path(p) = &*c.func
                    && let Some(seg) = p.path.segments.last()
                {
                    let name = seg.ident.to_string();
                    self.hit(Marker::Call(&name));
                }
            }
            syn::Expr::MethodCall(mc) => {
                let name = mc.method.to_string();
                self.hit(Marker::Call(&name));
            }
            _ => {}
        }
        syn::visit::visit_expr(self, node);
    }
}

fn positions_in(block: &syn::Block, markers: &[Marker<'_>]) -> Vec<Option<usize>> {
    let mut w = Walker {
        markers,
        found: vec![None; markers.len()],
        tick: 0,
    };
    syn::visit::visit_block(&mut w, block);
    w.found
}

macro_rules! order_queries {
    ($t:ty) => {
        impl $t {
            /// Pre-order position of the first occurrence of each marker, in one traversal.
            ///
            /// `None` for a marker that does not occur. Positions are comparable to each
            /// other and meaningless in isolation — do not assert on the number.
            pub fn positions(&self, markers: &[Marker<'_>]) -> Vec<Option<usize>> {
                positions_in(self.block(), markers)
            }

            /// Position of one marker, or `None`.
            pub fn position(&self, marker: Marker<'_>) -> Option<usize> {
                positions_in(self.block(), &[marker])[0]
            }

            /// `true` when every marker occurs and they occur in the order given.
            ///
            /// A **missing** marker makes this `false` rather than "vacuously ordered" —
            /// an ordering claim about something that is not there is not satisfied, it is
            /// unanswerable, and answering `true` is how a guard goes quietly vacuous.
            /// Use [`Self::explain_order`] to report which one is missing.
            pub fn in_order(&self, markers: &[Marker<'_>]) -> bool {
                let p = positions_in(self.block(), markers);
                p.iter().all(|x| x.is_some()) && p.windows(2).all(|w| w[0] < w[1])
            }

            /// A human-readable rendering of where each marker landed, for failure
            /// messages. Names the missing ones explicitly.
            pub fn explain_order(&self, markers: &[Marker<'_>]) -> String {
                positions_in(self.block(), markers)
                    .iter()
                    .zip(markers)
                    .map(|(pos, m)| match pos {
                        Some(p) => format!("{m:?}@{p}"),
                        None => format!("{m:?}=ABSENT"),
                    })
                    .collect::<Vec<_>>()
                    .join(" -> ")
            }
        }
    };
}

order_queries!(FnScope<'_>);
order_queries!(MethodScope<'_>);
