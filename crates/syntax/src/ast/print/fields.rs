use super::super::children::Child;
use super::super::tree::{NumberKind, Tree};
use super::{Printer, quote};
use crate::names::to_utf16;
use std::fmt::Write;

impl Printer<'_> {
    /// Print the fields of `tree`.
    pub fn fields(&mut self, tree: &Tree) {
        let ast = self.ast;
        use Tree::*;
        match tree {
            Ident { name } => self.name(*name),
            Select { qualifier, name } => {
                self.node(*qualifier);
                self.name(*name);
            }
            This { qual } => self.opt(qual, "_tident"),
            Super { qual, mix } => {
                self.node(*qual);
                self.opt(mix, "_tident");
            }
            Apply { fun, args } | TypeApply { fun, args } => {
                self.node(*fun);
                self.list(*args);
            }
            Literal { value } => self.constant(value),
            NamedArg { name, arg } => {
                self.name(*name);
                self.node(*arg);
            }
            Block { stats, expr } => {
                self.list(*stats);
                self.opt(expr, "_");
            }
            Match { selector, cases, .. } => {
                self.opt(selector, "_");
                self.list(*cases);
            }
            Return { expr } => {
                self.opt(expr, "_");
                self.out.push_str(" _");
            }
            Quote { body } => {
                self.node(*body);
                self.out.push_str(" []");
            }
            SplicePattern { body } => {
                self.node(*body);
                self.out.push_str(" [] []");
            }
            Bind { name, body, .. } => {
                self.name(*name);
                self.node(*body);
            }
            ValDef { name, tpt, rhs, mods } => {
                self.mods(ast.modifiers(*mods));
                self.name(*name);
                self.node(*tpt);
                self.opt(rhs, "_");
            }
            DefDef { name, paramss, tpt, rhs, mods } => {
                self.mods(ast.modifiers(*mods));
                self.name(*name);
                self.clauses(*paramss);
                self.node(*tpt);
                self.opt(rhs, "_");
            }
            TypeDef { name, rhs, mods } | ModuleDef { name, imp: rhs, mods } => {
                self.mods(ast.modifiers(*mods));
                self.name(*name);
                self.node(*rhs);
            }
            Template { constr, parents_or_derived, self_, body, derived } => {
                if let Some(n) = derived {
                    write!(self.out, " derived={n}").unwrap();
                }
                self.node(*constr);
                self.list(*parents_or_derived);
                self.opt(self_, "_self");
                self.list(*body);
            }
            SymbolLit { value } => write!(self.out, " {}", quote(value)).unwrap(),
            TypedSplice { .. } => self.out.push_str(" <typed>"),
            InterpolatedString { id, segments } => {
                self.name(*id);
                self.list(*segments);
            }
            Function { args, body, mods, .. } => {
                if let Some((m, erased)) = mods {
                    self.mods(ast.modifiers(*m));
                    let e: Vec<&str> = erased.iter().map(|b| if *b { "true" } else { "false" }).collect();
                    write!(self.out, " erased=[{}]", e.join(" ")).unwrap();
                }
                self.list(*args);
                self.node(*body);
            }
            GenFrom { pat, expr, check_mode } => {
                self.node(*pat);
                self.node(*expr);
                write!(self.out, " {check_mode:?}").unwrap();
            }
            PatDef { mods, pats, tpt, rhs } => {
                self.mods(ast.modifiers(*mods));
                self.list(*pats);
                self.node(*tpt);
                self.opt(rhs, "_");
            }
            ExtMethods { paramss, methods } => {
                self.clauses(*paramss);
                self.list(*methods);
            }
            ContextBoundTypeTree { tycon, param_name, own_name } => {
                self.node(*tycon);
                self.name(*param_name);
                self.name(*own_name);
            }
            Number { digits, kind } => {
                let kind = match kind {
                    NumberKind::Whole(r) => format!("Whole({r})"),
                    k => format!("{k:?}"),
                };
                write!(self.out, " {} {kind}", quote(&to_utf16(digits))).unwrap();
            }
            _ => {
                for &c in ast.children_of(tree).iter() {
                    match c {
                        Child::Node(n) => self.node(n),
                        Child::Empty => self.out.push_str(" _"),
                        Child::List(l) => self.list(l),
                        Child::Clauses(c) => self.clauses(c),
                        Child::Mods(m) => self.mods(ast.modifiers(m)),
                    }
                }
            }
        }
    }
}
