use std::collections::BTreeSet;

use quote::ToTokens;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit::Visit;

use crate::scope::{render_type, FnScope, MethodScope, ScopeError};
use crate::RustSource;

pub(crate) fn macro_exprs(mac: &syn::Macro) -> Vec<syn::Expr> {
    let tokens = mac.tokens.clone();
    if let Ok(p) = Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated.parse2(tokens.clone()) {
        return p.into_iter().collect();
    }
    if let Ok(e) = syn::parse2::<syn::Expr>(tokens) {
        return vec![e];
    }
    Vec::new()
}

pub(crate) fn token_idents(tokens: &proc_macro2::TokenStream, name: &str) -> usize {
    let mut n = 0;
    for tt in tokens.clone() {
        match tt {
            proc_macro2::TokenTree::Ident(i) => {
                if i == name {
                    n += 1;
                }
            }
            proc_macro2::TokenTree::Group(g) => n += token_idents(&g.stream(), name),
            _ => {}
        }
    }
    n
}

fn token_str_lits(tokens: &proc_macro2::TokenStream, out: &mut Vec<String>) {
    for tt in tokens.clone() {
        match tt {
            proc_macro2::TokenTree::Literal(l) => {
                if let syn::Lit::Str(s) = syn::Lit::new(l) {
                    out.push(s.value());
                }
            }
            proc_macro2::TokenTree::Group(g) => token_str_lits(&g.stream(), out),
            _ => {}
        }
    }
}

pub(crate) fn render_path(p: &syn::Path) -> String {
    p.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

struct ExprWalker<'f> {
    on_expr: &'f mut dyn FnMut(&syn::Expr),
    on_type_path: &'f mut dyn FnMut(&syn::TypePath),
}

impl<'ast, 'f> Visit<'ast> for ExprWalker<'f> {
    fn visit_expr(&mut self, node: &'ast syn::Expr) {
        (self.on_expr)(node);
        syn::visit::visit_expr(self, node);
    }

    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        (self.on_type_path)(node);
        syn::visit::visit_type_path(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        for e in macro_exprs(node) {
            let mut inner = ExprWalker {
                on_expr: self.on_expr,
                on_type_path: self.on_type_path,
            };
            inner.visit_expr(&e);
        }
    }
}

fn walk_exprs(
    file: &syn::File,
    mut on_expr: impl FnMut(&syn::Expr),
    mut on_type_path: impl FnMut(&syn::TypePath),
) {
    let mut w = ExprWalker {
        on_expr: &mut on_expr,
        on_type_path: &mut on_type_path,
    };
    w.visit_file(file);
}

fn walk_block_exprs(block: &syn::Block, mut on_expr: impl FnMut(&syn::Expr)) {
    let mut noop = |_: &syn::TypePath| {};
    let mut w = ExprWalker {
        on_expr: &mut on_expr,
        on_type_path: &mut noop,
    };
    w.visit_block(block);
}

struct IdentWalker<'n> {
    name: &'n str,
    count: usize,
}

impl<'ast, 'n> Visit<'ast> for IdentWalker<'n> {
    fn visit_ident(&mut self, node: &'ast proc_macro2::Ident) {
        if node == self.name {
            self.count += 1;
        }
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.count += token_idents(&node.tokens, self.name);
    }
}

struct AttrWalker<'n> {
    path: &'n str,
    count: usize,
}

impl<'ast, 'n> Visit<'ast> for AttrWalker<'n> {
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        if node.path().segments.last().is_some_and(|s| s.ident == self.path) {
            self.count += 1;
        }
        syn::visit::visit_attribute(self, node);
    }
}

struct LitWalker {
    out: Vec<String>,
}

impl<'ast> Visit<'ast> for LitWalker {
    fn visit_lit_str(&mut self, node: &'ast syn::LitStr) {
        self.out.push(node.value());
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        token_str_lits(&node.tokens, &mut self.out);
    }
}

fn derive_names(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut out = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("derive") {
            continue;
        }
        if let Ok(list) =
            attr.parse_args_with(Punctuated::<syn::Path, syn::Token![,]>::parse_terminated)
        {
            out.extend(
                list.iter()
                    .filter_map(|p| p.segments.last().map(|s| s.ident.to_string())),
            );
        }
    }
    out
}

fn item_attrs(item: &syn::Item) -> Option<&[syn::Attribute]> {
    match item {
        syn::Item::Struct(s) => Some(&s.attrs),
        syn::Item::Enum(e) => Some(&e.attrs),
        syn::Item::Union(u) => Some(&u.attrs),
        syn::Item::Type(t) => Some(&t.attrs),
        _ => None,
    }
}

fn has_attr(attrs: &[syn::Attribute], path: &str) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == path))
}

fn path_root_if_crate_like(p: &syn::Path) -> Option<String> {
    if p.segments.len() < 2 {
        return None;
    }
    let first = p.segments.first()?.ident.to_string();
    let starts_lower = first
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_');
    if starts_lower && first != "self" && first != "super" && first != "crate" {
        Some(first)
    } else {
        None
    }
}

fn arm_paths(pat: &syn::Pat, out: &mut Vec<String>) {
    match pat {
        syn::Pat::Path(p) => out.push(render_path(&p.path)),
        syn::Pat::TupleStruct(t) => out.push(render_path(&t.path)),
        syn::Pat::Struct(s) => out.push(render_path(&s.path)),
        syn::Pat::Ident(i) => {
            out.push(i.ident.to_string());
            if let Some((_, sub)) = &i.subpat {
                arm_paths(sub, out);
            }
        }
        syn::Pat::Or(o) => {
            for c in &o.cases {
                arm_paths(c, out);
            }
        }
        syn::Pat::Reference(r) => arm_paths(&r.pat, out),
        syn::Pat::Paren(p) => arm_paths(&p.pat, out),
        _ => {}
    }
}

fn block_match_facts(block: &syn::Block) -> (bool, Vec<String>) {
    let mut wildcard = false;
    let mut paths = Vec::new();
    walk_block_exprs(block, |e| {
        if let syn::Expr::Match(m) = e {
            for arm in &m.arms {
                if matches!(arm.pat, syn::Pat::Wild(_)) {
                    wildcard = true;
                }
                arm_paths(&arm.pat, &mut paths);
            }
        }
    });
    (wildcard, paths)
}

impl RustSource {
    pub fn file_call_count(&self, name: &str) -> usize {
        let mut n = 0;
        walk_exprs(
            self.ast(),
            |e| match e {
                syn::Expr::Call(c) => {
                    if let syn::Expr::Path(p) = &*c.func
                        && p.path.segments.last().is_some_and(|s| s.ident == name)
                    {
                        n += 1;
                    }
                }
                syn::Expr::MethodCall(m) if m.method == name => n += 1,
                _ => {}
            },
            |_| {},
        );
        n
    }

    pub fn file_calls_path_with_str_arg(&self, path_suffix: &str, lit: &str) -> usize {
        let mut n = 0;
        walk_exprs(
            self.ast(),
            |e| {
                if let syn::Expr::Call(c) = e
                    && let syn::Expr::Path(p) = &*c.func
                    && render_path(&p.path).ends_with(path_suffix)
                    && let Some(syn::Expr::Lit(first)) = c.args.first()
                    && let syn::Lit::Str(s) = &first.lit
                    && s.value() == lit
                {
                    n += 1;
                }
            },
            |_| {},
        );
        n
    }

    pub fn file_field_read_count(&self, name: &str) -> usize {
        let mut n = 0;
        walk_exprs(
            self.ast(),
            |e| {
                if let syn::Expr::Field(f) = e
                    && let syn::Member::Named(id) = &f.member
                    && id == name
                {
                    n += 1;
                }
            },
            |_| {},
        );
        n
    }

    pub fn ident_count(&self, name: &str) -> usize {
        let mut w = IdentWalker { name, count: 0 };
        w.visit_file(self.ast());
        w.count
    }

    pub fn free_fn_names(&self) -> Vec<String> {
        self.ast()
            .items
            .iter()
            .filter_map(|it| match it {
                syn::Item::Fn(f) => Some(f.sig.ident.to_string()),
                _ => None,
            })
            .collect()
    }

    pub fn structs_declaring_field(&self, name: &str) -> Vec<String> {
        self.ast()
            .items
            .iter()
            .filter_map(|it| match it {
                syn::Item::Struct(s)
                    if s.fields
                        .iter()
                        .any(|f| f.ident.as_ref().is_some_and(|i| i == name)) =>
                {
                    Some(s.ident.to_string())
                }
                _ => None,
            })
            .collect()
    }

    pub fn uses(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for item in &self.ast().items {
            if let syn::Item::Use(u) = item {
                out.append(&mut RustSource::use_roots_of(&u.tree));
            }
        }
        let mut from_exprs = BTreeSet::new();
        let mut from_types = BTreeSet::new();
        walk_exprs(
            self.ast(),
            |e| {
                if let syn::Expr::Path(p) = e
                    && let Some(root) = path_root_if_crate_like(&p.path)
                {
                    from_exprs.insert(root);
                }
            },
            |t| {
                if let Some(root) = path_root_if_crate_like(&t.path) {
                    from_types.insert(root);
                }
            },
        );
        out.append(&mut from_exprs);
        out.append(&mut from_types);
        out.retain(|r| r != "self" && r != "super" && r != "crate");
        out
    }

    fn use_roots_of(tree: &syn::UseTree) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        match tree {
            syn::UseTree::Path(p) => {
                out.insert(p.ident.to_string());
            }
            syn::UseTree::Name(n) => {
                out.insert(n.ident.to_string());
            }
            syn::UseTree::Rename(r) => {
                out.insert(r.ident.to_string());
            }
            syn::UseTree::Group(g) => {
                for t in &g.items {
                    out.append(&mut RustSource::use_roots_of(t));
                }
            }
            syn::UseTree::Glob(_) => {}
        }
        out
    }

    pub fn derives(&self, struct_name: &str) -> Result<Vec<String>, ScopeError> {
        let s = self.struct_named(struct_name)?;
        Ok(derive_names(&s.attrs))
    }

    pub fn any_derive(&self, name: &str) -> bool {
        self.ast()
            .items
            .iter()
            .filter_map(item_attrs)
            .any(|attrs| derive_names(attrs).iter().any(|d| d == name))
    }

    pub fn attr_count(&self, path: &str) -> usize {
        let mut w = AttrWalker { path, count: 0 };
        w.visit_file(self.ast());
        w.count
    }

    pub fn fns_with_attr(&self, path: &str) -> Vec<String> {
        let mut out = Vec::new();
        for item in &self.ast().items {
            match item {
                syn::Item::Fn(f) if has_attr(&f.attrs, path) => out.push(f.sig.ident.to_string()),
                syn::Item::Impl(imp) => {
                    for ii in &imp.items {
                        if let syn::ImplItem::Fn(m) = ii
                            && has_attr(&m.attrs, path)
                        {
                            out.push(format!("{}::{}", render_type(&imp.self_ty), m.sig.ident));
                        }
                    }
                }
                syn::Item::Mod(m) => {
                    if let Some((_, items)) = &m.content {
                        for it in items {
                            if let syn::Item::Fn(f) = it
                                && has_attr(&f.attrs, path)
                            {
                                out.push(f.sig.ident.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    pub fn const_expr(&self, name: &str) -> Result<String, ScopeError> {
        let mut available = Vec::new();
        for item in &self.ast().items {
            if let syn::Item::Const(c) = item {
                if c.ident == name {
                    return Ok(c.expr.to_token_stream().to_string());
                }
                available.push(c.ident.to_string());
            }
        }
        Err(self.scope_error(format!("const `{name}`"), available))
    }

    pub fn const_u64(&self, name: &str) -> Result<u64, ScopeError> {
        for item in &self.ast().items {
            if let syn::Item::Const(c) = item
                && c.ident == name
            {
                return match &*c.expr {
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Int(i),
                        ..
                    }) => i.base10_parse::<u64>().map_err(|e| {
                        self.scope_error(format!("const `{name}` as u64 ({e})"), vec![])
                    }),
                    other => Err(self.scope_error(
                        format!(
                            "const `{name}` as an integer literal (it is `{}`)",
                            other.to_token_stream()
                        ),
                        vec![],
                    )),
                };
            }
        }
        self.const_expr(name).map(|_| unreachable!("const_expr found what const_u64 did not"))
    }

    pub fn string_literals(&self) -> Vec<String> {
        let mut w = LitWalker { out: Vec::new() };
        w.visit_file(self.ast());
        w.out
    }
}

struct SiteWalker<'n> {
    method: &'n str,
    stack: Vec<String>,
    hits: std::collections::BTreeMap<String, usize>,
}

impl<'n> SiteWalker<'n> {
    fn record(&mut self) {
        let key = self.stack.last().cloned().unwrap_or_else(|| "<file>".to_string());
        *self.hits.entry(key).or_insert(0) += 1;
    }
}

impl<'ast, 'n> Visit<'ast> for SiteWalker<'n> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.stack.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.stack.pop();
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == self.method {
            self.record();
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        for e in macro_exprs(node) {
            let mut inner = SiteWalker {
                method: self.method,
                stack: self.stack.clone(),
                hits: std::collections::BTreeMap::new(),
            };
            inner.visit_expr(&e);
            for (k, v) in inner.hits {
                *self.hits.entry(k).or_insert(0) += v;
            }
        }
    }
}

fn strip_parens(e: &syn::Expr) -> &syn::Expr {
    match e {
        syn::Expr::Paren(p) => strip_parens(&p.expr),
        other => other,
    }
}

impl RustSource {
    pub fn method_call_sites(&self, method: &str) -> std::collections::BTreeMap<String, usize> {
        let mut w = SiteWalker {
            method,
            stack: Vec::new(),
            hits: std::collections::BTreeMap::new(),
        };
        w.visit_file(self.ast());
        w.hits
    }

    pub fn negated_method_call_count(&self, method: &str) -> usize {
        let mut n = 0;
        walk_exprs(
            self.ast(),
            |e| {
                if let syn::Expr::Unary(u) = e
                    && matches!(u.op, syn::UnOp::Not(_))
                    && let syn::Expr::MethodCall(m) = strip_parens(&u.expr)
                    && m.method == method
                {
                    n += 1;
                }
            },
            |_| {},
        );
        n
    }
}

impl<'a> FnScope<'a> {
    pub fn match_has_wildcard_arm(&self) -> bool {
        block_match_facts(self.block).0
    }

    pub fn match_arm_paths(&self) -> Vec<String> {
        block_match_facts(self.block).1
    }
}

impl<'a> MethodScope<'a> {
    pub fn match_has_wildcard_arm(&self) -> bool {
        block_match_facts(self.block).0
    }

    pub fn match_arm_paths(&self) -> Vec<String> {
        block_match_facts(self.block).1
    }
}
