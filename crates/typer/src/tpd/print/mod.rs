//! Typed trees as S-expressions, in the format of `scripts/typer-oracle`.

pub mod symbols;
pub mod types;

use super::{Clauses, List, TNodeId, Tpd, Tree};
use std::fmt::Write;
use syntax::ast::TName;
use syntax::ast::constant::Constant;
use syntax::ast::print::{quote, span};
use syntax::names::{Names, to_utf16};
use tasty::Universe;

pub struct Printer<'a> {
    pub tpd: &'a Tpd,
    pub names: &'a Names,
    pub u: &'a mut Universe,
    pub out: String,
}

impl Printer<'_> {
    fn name(&mut self, n: TName) {
        let q = quote(&to_utf16(self.names.text(n.name)));
        write!(self.out, " {}:{}", if n.is_type { "T" } else { "t" }, &q[1..q.len() - 1]).unwrap();
    }

    fn opt(&mut self, t: Option<TNodeId>) {
        self.out.push(' ');
        match t {
            Some(t) => self.tree(t),
            None => self.out.push('_'),
        }
    }

    fn node(&mut self, t: TNodeId) {
        self.opt(Some(t));
    }

    fn list(&mut self, l: List) {
        self.out.push_str(" [");
        for &t in self.tpd.items(l) {
            self.node(t);
        }
        self.out.push(']');
    }

    fn clauses(&mut self, c: Clauses) {
        self.out.push_str(" [");
        for &l in self.tpd.clause_items(c) {
            self.list(l);
        }
        self.out.push(']');
    }

    fn constant(&mut self, c: &Constant) {
        match c {
            Constant::Unit | Constant::Null => write!(self.out, " #{}", c.tag()).unwrap(),
            _ => write!(self.out, " #{}:{}", c.tag(), quote(&c.string_value())).unwrap(),
        }
    }

    /// Print tree `id`.
    pub fn tree(&mut self, id: TNodeId) {
        let node = self.tpd.get(id);
        let tree = node.tree.clone();
        write!(self.out, "({} {}", class_name(&tree), span(node.span)).unwrap();
        if let Some(s) = node.sym {
            match tree {
                Tree::Template { .. } => {
                    let full = symbols::full_name(self.u, s);
                    let name = &self.u.symbol(s).name;
                    let local = format!("{full}.<local {name}>");
                    write!(self.out, " sym={}", quote(&local.encode_utf16().collect::<Vec<_>>())).unwrap();
                }
                _ => write!(self.out, " sym={}", symbols::sym(self.u, s)).unwrap(),
            }
        }
        if matches!(tree, Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. }) {
            let flags = node.sym.map(|s| self.u.symbol(s).flags).unwrap_or_default();
            write!(self.out, " [{}]", flag_names(flags)).unwrap();
        }
        if let Some(t) = node.tpe {
            write!(self.out, " :{}", types::tpe(self.u, t)).unwrap();
        }
        self.fields(&tree);
        self.out.push(')');
    }

    fn fields(&mut self, tree: &Tree) {
        match *tree {
            Tree::Ident { name } => self.name(name),
            Tree::Select { qualifier, name } => {
                self.node(qualifier);
                self.name(name);
            }
            Tree::This { qual } => self.node(qual),
            Tree::Super { qual, mix } => {
                self.node(qual);
                match mix {
                    Some(m) => self.name(m),
                    None => self.out.push_str(" _"),
                }
            }
            Tree::Apply { fun, args } | Tree::TypeApply { fun, args } => {
                self.node(fun);
                self.list(args);
            }
            Tree::Literal { ref value } => self.constant(value),
            Tree::ClassLiteral { tpe } => write!(self.out, " #Class:{}", types::tpe(self.u, tpe)).unwrap(),
            Tree::New { tpt } => self.node(tpt),
            Tree::Typed { expr: a, tpt: b } | Tree::Assign { lhs: a, rhs: b } => {
                self.node(a);
                self.node(b);
            }
            Tree::Block { stats, expr } => {
                self.list(stats);
                self.node(expr);
            }
            Tree::If { cond, thenp, elsep } => {
                self.node(cond);
                self.node(thenp);
                self.node(elsep);
            }
            Tree::CaseDef { pat, guard, body } => {
                self.node(pat);
                self.opt(guard);
                self.node(body);
            }
            Tree::WhileDo { cond, body } => {
                self.node(cond);
                self.node(body);
            }
            Tree::Try { expr, cases, finalizer } => {
                self.node(expr);
                self.list(cases);
                self.opt(finalizer);
            }
            Tree::TypeTree { .. } => {}
            Tree::SingletonTypeTree { reference } => self.node(reference),
            Tree::AppliedTypeTree { tpt, args } => {
                self.node(tpt);
                self.list(args);
            }
            Tree::Bind { name, body } => {
                self.name(name);
                self.node(body);
            }
            Tree::ValDef { name, tpt, rhs } => {
                self.name(name);
                self.node(tpt);
                self.opt(rhs);
            }
            Tree::DefDef { name, paramss, tpt, rhs } => {
                self.name(name);
                self.clauses(paramss);
                self.node(tpt);
                self.opt(rhs);
            }
            Tree::TypeDef { name, rhs } => {
                self.name(name);
                self.node(rhs);
            }
            Tree::Template { constr, parents, self_, body } => {
                self.node(constr);
                self.list(parents);
                match self_ {
                    Some(s) => self.node(s),
                    None => self.out.push_str(" _self"),
                }
                self.list(body);
            }
            Tree::PackageDef { pid, stats } => {
                self.node(pid);
                self.list(stats);
            }
        }
    }
}

/// The TASTy modifier names of `flags`, by tag number.
fn flag_names(flags: tasty::Flags) -> String {
    let tags = (0..64u8).filter(|&t| tasty::tags::is_modifier(t) && flags.has(t));
    tags.map(tasty::tags::name).collect::<Vec<_>>().join(" ")
}

/// dotty's class name of `tree`.
fn class_name(tree: &Tree) -> String {
    match tree {
        Tree::TypeTree { inferred: true } => "InferredTypeTree".into(),
        Tree::ClassLiteral { .. } => "Literal".into(),
        t => {
            let s = format!("{t:?}");
            s[..s.find([' ', '{', '(']).unwrap_or(s.len())].to_string()
        }
    }
}
