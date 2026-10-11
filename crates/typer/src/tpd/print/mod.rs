//! Typed trees as S-expressions, in the format of `scripts/typer-oracle`.

pub mod symbols;
pub mod types;

use super::{Clauses, Field, List, TNodeId, Tpd, Tree};
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

    /// The fields of `tree`, an empty `Template.self` (its only optional field) as dotty's
    /// `EmptyValDef`.
    fn fields(&mut self, tree: &mut Tree) {
        let template = matches!(tree, Tree::Template { .. });
        tree.fields(&mut |field| match field {
            Field::Node(t) => self.node(*t),
            Field::Opt(None) if template => self.out.push_str(" _self"),
            Field::Opt(t) => self.opt(*t),
            Field::List(l) => self.list(*l),
            Field::Clauses(c) => self.clauses(*c),
            Field::Name(n) => self.name(n),
            Field::OptName(Some(n)) => self.name(n),
            Field::OptName(None) => self.out.push_str(" _"),
            Field::Constant(c) => self.constant(c),
            Field::Class(tpe) => {
                let shown = types::tpe(self.u, tpe);
                self.out.push_str(" #Class:");
                self.out.push_str(&shown);
            }
        });
    }

    /// Print tree `id`.
    pub fn tree(&mut self, id: TNodeId) {
        let node = self.tpd.get(id);
        let mut tree = node.tree.clone();
        if let Tree::Ident { name } = tree
            && self.names.text(name.name).is_empty()
            && name.is_type
        {
            self.out.push_str("(EmptyTypeIdent$ - T:)");
            return;
        }
        write!(self.out, "({} {}", class_name(&tree), span(node.span)).unwrap();
        if let Tree::Ident { name } = tree
            && node.sym == Some(self.u.root())
            && self.names.text(name.name) == "_root_"
        {
            self.out.push_str(" sym=\"_root_\" :t:_root_");
            self.fields(&mut tree);
            self.out.push(')');
            return;
        }
        if let Some(s) = node.sym {
            match tree {
                Tree::Template { .. } => {
                    let s = if self.u.is_class(s) { s } else { self.u.symbol(s).owner.expect("a class") };
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
        match tree {
            Tree::ClassLiteral { tpe } => write!(self.out, " :#Class:{}", types::tpe(self.u, tpe)).unwrap(),
            Tree::Alternative { .. } | Tree::NamedArg { .. } | Tree::SeqLiteral { .. } => {}
            _ => {
                if let Some(t) = node.tpe {
                    write!(self.out, " :{}", types::tpe(self.u, t)).unwrap();
                }
            }
        }
        self.fields(&mut tree);
        self.out.push(')');
    }
}

/// The TASTy modifier names of `flags`, by tag number.
fn flag_names(flags: tasty::Flags) -> String {
    let tags = (0..64u8).filter(|&t| tasty::tags::is_modifier(t) && flags.has(t));
    tags.map(tasty::tags::name).collect::<Vec<_>>().join(" ")
}

/// The name of `tree`'s class in dotty.
pub fn class_name(tree: &Tree) -> String {
    match tree {
        Tree::TypeTree { inferred: true } => "InferredTypeTree".into(),
        Tree::ClassLiteral { .. } => "Literal".into(),
        t => {
            let s = format!("{t:?}");
            s[..s.find([' ', '{', '(']).unwrap_or(s.len())].to_string()
        }
    }
}
