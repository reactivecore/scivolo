mod inline;

use crate::names::{NameRef, Names};
use crate::tags::*;
use crate::tree::{Ast, NodeId};
use crate::view::{View, view};
use inline::{Inline, is_inline};
use std::fmt::Write;

const WIDTH: usize = 110;

/// Indented dump of an ASTs section: statements and terms one per line, types and paths inline.
pub fn dump(ast: &Ast, names: &Names) -> String {
    let p = Printer { i: Inline { ast, names }, out: String::new() };
    let mut p = p;
    for &r in ast.roots() {
        p.tree(r, 0);
    }
    p.out
}

struct Printer<'x> {
    i: Inline<'x>,
    out: String,
}

impl Printer<'_> {
    fn line(&mut self, depth: usize, text: &str) {
        let _ = writeln!(self.out, "{:w$}{text}", "", w = depth * 2);
    }

    fn mods(&self, ids: &[NodeId]) -> String {
        ids.iter().map(|&m| format!(" {}", self.i.render(m))).collect()
    }

    fn head(&self, id: NodeId) -> String {
        let ast = self.i.ast;
        let n = ast.node(id);
        let tag = name(n.tag);
        match view(ast, id) {
            View::TypeDef { name, rhs, is_class, mods } => {
                let kind = if is_class { "CLASSDEF" } else { "TYPEDEF" };
                let rhs = if is_class { String::new() } else { format!(" {}", self.i.render(rhs)) };
                format!("{kind} {}@{}{}{rhs}", self.nm(name), n.addr, self.mods(mods))
            }
            View::DefDef { name, parts } => {
                format!("DEFDEF {}@{}{}", self.nm(name), n.addr, self.mods(parts.mods))
            }
            View::ValDef { name, tpt, mods, .. } => {
                format!("VALDEF {}@{}: {}{}", self.nm(name), n.addr, self.i.render(tpt), self.mods(mods))
            }
            View::Param { name, tpt, mods } => {
                format!("{tag} {}@{}: {}{}", self.nm(name), n.addr, self.i.render(tpt), self.mods(mods))
            }
            View::Ident { name, tpe } => format!("IDENT {}: {}", self.nm(name), self.i.render(tpe)),
            View::Select { name, .. } => format!("{tag} {}", self.nm(name)),
            View::If { inline: true, .. } => "IF inline".into(),
            View::Match { prefix: Some(p), .. } => format!("MATCH {}", name(p).to_lowercase()),
            View::Const(c) => format!("CONST {}", self.i.constant(c)),
            View::Shared(a) => {
                let target = ast.at(a).map_or("?", |t| name(ast.tag(t)));
                format!("{tag} -> @{a} {target}")
            }
            _ => match n.tag {
                TERMREFdirect | TYPEREFdirect => format!("{tag} {}", self.i.sym(n.a)),
                RETURN => format!("RETURN from {}", self.i.sym(n.a)),
                BIND => format!("BIND {}@{}", self.i.name(n.a), n.addr),
                NAMEDARG | SELFDEF | SELECTin => format!("{tag} {}", self.i.name(n.a)),
                TEMPLATE => format!("TEMPLATE @{}", n.addr),
                _ if leading_nats(n.tag) > 0 => format!("{tag} {}", n.a),
                _ => tag.into(),
            },
        }
    }

    fn nm(&self, r: NameRef) -> String {
        self.i.names.show(r)
    }

    /// Children shown under the head line (the rest are folded into the head).
    fn body<'a>(&self, id: NodeId, kids: &'a [NodeId]) -> &'a [NodeId] {
        match view(self.i.ast, id) {
            View::TypeDef { is_class: true, .. } => &kids[..1],
            View::TypeDef { .. } | View::Param { .. } | View::Ident { .. } | View::Shared(_) => &[],
            View::DefDef { parts, .. } => &kids[..parts.params.len() + parts.body.len()],
            View::ValDef { rhs, .. } => rhs.map_or(&[], |_| &kids[1..2]),
            View::If { inline: true, .. } | View::Match { prefix: Some(_), .. } => &kids[1..],
            _ => kids,
        }
    }

    fn tree(&mut self, id: NodeId, depth: usize) {
        let ast = self.i.ast;
        let tag = ast.tag(id);
        if is_inline(tag) {
            let text = self.i.render(id);
            self.line(depth, &text);
            return;
        }
        let head = self.head(id);
        let kids = self.body(id, ast.kids(id));
        if kids.iter().all(|&k| is_inline(ast.tag(k))) {
            let parts: Vec<_> = kids.iter().map(|&k| self.i.render(k)).collect();
            let one = format!("{head} {}", parts.join(" "));
            if one.len() + depth * 2 <= WIDTH {
                self.line(depth, one.trim_end());
                return;
            }
        }
        self.line(depth, &head);
        for &k in kids {
            self.tree(k, depth + 1);
        }
    }
}
