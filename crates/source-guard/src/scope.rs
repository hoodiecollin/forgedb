use std::fmt;

use crate::RustSource;

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
            write!(f, " — available: {}", self.available.join(", "))
        }
    }
}

impl std::error::Error for ScopeError {}

pub struct FnScope<'a> {
    pub(crate) item: &'a syn::ItemFn,
    pub(crate) sig: &'a syn::Signature,
    pub(crate) block: &'a syn::Block,
    origin: String,
}

pub struct MethodScope<'a> {
    pub(crate) sig: &'a syn::Signature,
    pub(crate) block: &'a syn::Block,
    origin: String,
}

macro_rules! body_queries {
    ($t:ty) => {
        impl<'a> $t {
            pub fn block(&self) -> &'a syn::Block {
                self.block
            }

            pub fn sig(&self) -> &'a syn::Signature {
                self.sig
            }

            pub fn call_count(&self, name: &str) -> usize {
                let mut v = CallCounter {
                    name,
                    count: 0,
                };
                syn::visit::visit_block(&mut v, self.block);
                v.count
            }

            pub fn calls(&self, name: &str) -> bool {
                self.call_count(name) > 0
            }

            pub fn field_read_count(&self, name: &str) -> usize {
                let mut v = FieldReadCounter { name, count: 0 };
                syn::visit::visit_block(&mut v, self.block);
                v.count
            }

            pub fn stmt_index_of(&self, pred: impl Fn(&syn::Stmt) -> bool) -> Option<usize> {
                self.block.stmts.iter().position(pred)
            }

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

            pub fn body_text_because(&self, why: &str) -> String {
                debug_assert!(
                    !why.trim().is_empty(),
                    "body_text_because needs a real reason, not an empty string"
                );
                use quote::ToTokens;
                self.block.to_token_stream().to_string()
            }

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
    pub fn item(&self) -> &'a syn::ItemFn {
        self.item
    }
}

struct CallCounter<'n> {
    name: &'n str,
    count: usize,
}

impl<'ast, 'n> syn::visit::Visit<'ast> for CallCounter<'n> {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
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

impl RustSource {
    pub(crate) fn scope_error(&self, what: impl Into<String>, available: Vec<String>) -> ScopeError {
        ScopeError {
            what: what.into(),
            origin: self.origin().to_string(),
            available,
        }
    }

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
