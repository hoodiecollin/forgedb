use crate::scope::{FnScope, MethodScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker<'a> {
    Call(&'a str),
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
            pub fn positions(&self, markers: &[Marker<'_>]) -> Vec<Option<usize>> {
                positions_in(self.block(), markers)
            }

            pub fn position(&self, marker: Marker<'_>) -> Option<usize> {
                positions_in(self.block(), &[marker])[0]
            }

            pub fn in_order(&self, markers: &[Marker<'_>]) -> bool {
                let p = positions_in(self.block(), markers);
                p.iter().all(|x| x.is_some()) && p.windows(2).all(|w| w[0] < w[1])
            }

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
