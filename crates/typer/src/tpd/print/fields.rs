//! The fields of each tree class, in the order of dotty's case classes.

use super::Printer;
use crate::tpd::Tree;

impl Printer<'_> {
    pub fn fields(&mut self, tree: &Tree) {
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
            Tree::ClassLiteral { tpe } => {
                let shown = super::types::tpe(self.u, tpe);
                self.out.push_str(" #Class:");
                self.out.push_str(&shown);
            }
            Tree::New { tpt } => self.node(tpt),
            Tree::SeqLiteral { elems, elemtpt } => {
                self.list(elems);
                self.node(elemtpt);
            }
            Tree::Typed { expr: a, tpt: b }
            | Tree::Assign { lhs: a, rhs: b }
            | Tree::Annotated { arg: a, annot: b } => {
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
            Tree::Return { expr, from } => {
                self.node(expr);
                self.node(from);
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
            Tree::SingletonTypeTree { reference: a } | Tree::ByNameTypeTree { result: a } => self.node(a),
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
            Tree::TypeBoundsTree { lo, hi, alias } => {
                self.node(lo);
                self.node(hi);
                self.opt(alias);
            }
            Tree::Import { expr, selectors } => {
                self.node(expr);
                self.list(selectors);
            }
            Tree::ImportSelector { imported, renamed, bound } => {
                self.node(imported);
                self.opt(renamed);
                self.opt(bound);
            }
            Tree::PackageDef { pid, stats } => {
                self.node(pid);
                self.list(stats);
            }
        }
    }
}
