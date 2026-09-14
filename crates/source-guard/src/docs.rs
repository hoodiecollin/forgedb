use std::path::Path;

use quote::ToTokens;
use syn::spanned::Spanned;

use crate::RustSource;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocAttr {
    Literal(String),
    IncludeStr(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocSite {
    pub key: String,
    pub attr: Option<DocAttr>,
    pub line: usize,
}

fn start_line(attrs: &[syn::Attribute], vis: Option<&syn::Visibility>, ident: proc_macro2::Span) -> usize {
    let mut line = ident.start().line;
    if let Some(v) = vis {
        if !matches!(v, syn::Visibility::Inherited) {
            line = line.min(v.span().start().line);
        }
    }
    if let Some(first) = attrs.iter().find(|a| matches!(a.style, syn::AttrStyle::Outer)) {
        line = line.min(first.pound_token.span.start().line);
    }
    line
}

pub fn module_prefix_of(src_relative: &Path) -> Vec<String> {
    let mut parts: Vec<String> = src_relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if parts.first().is_some_and(|p| p == "src") {
        parts.remove(0);
    }
    let Some(last) = parts.pop() else {
        return Vec::new();
    };
    let stem = last.strip_suffix(".rs").unwrap_or(&last);
    if !matches!(stem, "lib" | "mod" | "main") {
        parts.push(stem.to_string());
    }
    parts
}

impl RustSource {
    pub fn doc_sites(&self, module_prefix: &[String]) -> Vec<DocSite> {
        let file = self.ast();
        let mut out = Vec::new();
        let root_key = if module_prefix.is_empty() {
            "crate".to_string()
        } else {
            module_prefix.join(".")
        };
        out.push(DocSite {
            key: root_key,
            attr: doc_attr_of(&file.attrs),
            line: 1,
        });
        let mut stack: Vec<String> = module_prefix.to_vec();
        for item in &file.items {
            visit_item(item, &mut stack, &mut out);
        }
        out
    }
}

fn key_for(stack: &[String], name: &str) -> String {
    if stack.is_empty() {
        name.to_string()
    } else {
        format!("{}.{name}", stack.join("."))
    }
}

fn is_public(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

fn wasm32_gated(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        if !a.path().is_ident("cfg") {
            return false;
        }
        let mut rendered = a.meta.to_token_stream().to_string();
        rendered.retain(|c| !c.is_whitespace());
        rendered.contains("target_arch=\"wasm32\"") && !rendered.contains("not(target_arch=\"wasm32\")")
    })
}

fn doc_attr_of(attrs: &[syn::Attribute]) -> Option<DocAttr> {
    let mut literal_lines: Vec<String> = Vec::new();
    let mut includes: Vec<String> = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        let syn::Meta::NameValue(nv) = &attr.meta else {
            continue;
        };
        match &nv.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) => literal_lines.push(strip_doc_space(&s.value())),
            syn::Expr::Macro(m) if m.mac.path.is_ident("include_str") => {
                match syn::parse2::<syn::LitStr>(m.mac.tokens.clone()) {
                    Ok(path) => includes.push(path.value()),
                    Err(_) => literal_lines.push(nv.value.to_token_stream().to_string()),
                }
            }
            other => literal_lines.push(other.to_token_stream().to_string()),
        }
    }
    if !literal_lines.is_empty() {
        return Some(DocAttr::Literal(literal_lines.join("\n")));
    }
    if includes.len() > 1 {
        return Some(DocAttr::Literal(includes.join(" + ")));
    }
    includes.pop().map(DocAttr::IncludeStr)
}

fn strip_doc_space(line: &str) -> String {
    line.strip_prefix(' ').unwrap_or(line).to_string()
}

fn push(
    out: &mut Vec<DocSite>,
    key: String,
    attrs: &[syn::Attribute],
    vis: Option<&syn::Visibility>,
    ident: proc_macro2::Span,
) {
    out.push(DocSite {
        key,
        attr: doc_attr_of(attrs),
        line: start_line(attrs, vis, ident),
    });
}

fn visit_fields(fields: &syn::Fields, owner: &str, out: &mut Vec<DocSite>) {
    if let syn::Fields::Named(named) = fields {
        for f in &named.named {
            if wasm32_gated(&f.attrs) || !is_public(&f.vis) {
                continue;
            }
            let Some(ident) = f.ident.as_ref() else { continue };
            push(out, format!("{owner}.{ident}"), &f.attrs, Some(&f.vis), ident.span());
        }
    }
}

fn self_type_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        syn::Type::Reference(r) => self_type_name(&r.elem),
        syn::Type::Group(g) => self_type_name(&g.elem),
        syn::Type::Paren(p) => self_type_name(&p.elem),
        _ => None,
    }
}

fn visit_item(item: &syn::Item, stack: &mut Vec<String>, out: &mut Vec<DocSite>) {
    match item {
        syn::Item::Fn(f) => {
            if wasm32_gated(&f.attrs) || !is_public(&f.vis) {
                return;
            }
            push(out, key_for(stack, &f.sig.ident.to_string()), &f.attrs, Some(&f.vis), f.sig.ident.span());
        }
        syn::Item::Struct(s) => {
            if wasm32_gated(&s.attrs) || !is_public(&s.vis) {
                return;
            }
            let key = key_for(stack, &s.ident.to_string());
            push(out, key.clone(), &s.attrs, Some(&s.vis), s.ident.span());
            visit_fields(&s.fields, &key, out);
        }
        syn::Item::Union(u) => {
            if wasm32_gated(&u.attrs) || !is_public(&u.vis) {
                return;
            }
            let key = key_for(stack, &u.ident.to_string());
            push(out, key.clone(), &u.attrs, Some(&u.vis), u.ident.span());
            visit_fields(&syn::Fields::Named(u.fields.clone()), &key, out);
        }
        syn::Item::Enum(e) => {
            if wasm32_gated(&e.attrs) || !is_public(&e.vis) {
                return;
            }
            let key = key_for(stack, &e.ident.to_string());
            push(out, key.clone(), &e.attrs, Some(&e.vis), e.ident.span());
            for v in &e.variants {
                if wasm32_gated(&v.attrs) {
                    continue;
                }
                let vkey = format!("{key}.{}", v.ident);
                push(out, vkey.clone(), &v.attrs, None, v.ident.span());
                if let syn::Fields::Named(named) = &v.fields {
                    for f in &named.named {
                        if wasm32_gated(&f.attrs) {
                            continue;
                        }
                        let Some(ident) = f.ident.as_ref() else { continue };
                        push(out, format!("{vkey}.{ident}"), &f.attrs, None, ident.span());
                    }
                }
            }
        }
        syn::Item::Type(t) => {
            if wasm32_gated(&t.attrs) || !is_public(&t.vis) {
                return;
            }
            push(out, key_for(stack, &t.ident.to_string()), &t.attrs, Some(&t.vis), t.ident.span());
        }
        syn::Item::Const(c) => {
            if wasm32_gated(&c.attrs) || !is_public(&c.vis) {
                return;
            }
            push(out, key_for(stack, &c.ident.to_string()), &c.attrs, Some(&c.vis), c.ident.span());
        }
        syn::Item::Static(s) => {
            if wasm32_gated(&s.attrs) || !is_public(&s.vis) {
                return;
            }
            push(out, key_for(stack, &s.ident.to_string()), &s.attrs, Some(&s.vis), s.ident.span());
        }
        syn::Item::Trait(t) => {
            if wasm32_gated(&t.attrs) || !is_public(&t.vis) {
                return;
            }
            let key = key_for(stack, &t.ident.to_string());
            push(out, key.clone(), &t.attrs, Some(&t.vis), t.ident.span());
            for it in &t.items {
                let (attrs, ident) = match it {
                    syn::TraitItem::Fn(f) => (&f.attrs, &f.sig.ident),
                    syn::TraitItem::Const(c) => (&c.attrs, &c.ident),
                    syn::TraitItem::Type(ty) => (&ty.attrs, &ty.ident),
                    _ => continue,
                };
                if wasm32_gated(attrs) {
                    continue;
                }
                push(out, format!("{key}.{ident}"), attrs, None, ident.span());
            }
        }
        syn::Item::Impl(i) => {
            if wasm32_gated(&i.attrs) || i.trait_.is_some() {
                return;
            }
            let Some(owner) = self_type_name(&i.self_ty) else {
                return;
            };
            let key = key_for(stack, &owner);
            for it in &i.items {
                let (attrs, vis, ident) = match it {
                    syn::ImplItem::Fn(f) => (&f.attrs, &f.vis, &f.sig.ident),
                    syn::ImplItem::Const(c) => (&c.attrs, &c.vis, &c.ident),
                    syn::ImplItem::Type(ty) => (&ty.attrs, &ty.vis, &ty.ident),
                    _ => continue,
                };
                if wasm32_gated(attrs) || !is_public(vis) {
                    continue;
                }
                push(out, format!("{key}.{ident}"), attrs, Some(vis), ident.span());
            }
        }
        syn::Item::Mod(m) => {
            if wasm32_gated(&m.attrs) || !is_public(&m.vis) {
                return;
            }
            let name = m.ident.to_string();
            push(out, key_for(stack, &name), &m.attrs, Some(&m.vis), m.ident.span());
            if let Some((_, items)) = &m.content {
                stack.push(name);
                for inner in items {
                    visit_item(inner, stack, out);
                }
                stack.pop();
            }
        }
        _ => {}
    }
}
