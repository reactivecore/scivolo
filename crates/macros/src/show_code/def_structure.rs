// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/Extractors.scala
//! Definitions and symbols as `Printer.TreeStructure` shows them.

use super::{Code, Shown};
use tasty::Kind;
use typer::tpd::{TNodeId, Tree as T};

impl Code<'_> {
    /// The structure of the `ValDef`, `DefDef` or `TypeDef` `t`.
    pub fn def_structure(&mut self, t: TNodeId) -> Shown {
        match self.tree_of(t) {
            T::ValDef { name, tpt, rhs } => {
                self.push(&format!("ValDef(\"{}\", ", self.name(name)));
                self.structure(tpt)?;
                self.rhs_structure(rhs)
            }
            T::DefDef { name, paramss, tpt, rhs } => {
                self.push(&format!("DefDef(\"{}\", ", self.name(name)));
                let clauses = self.h.tpd.clause_items(paramss).to_vec();
                self.push(if clauses.is_empty() { "Nil" } else { "List(" });
                for (i, &c) in clauses.iter().enumerate() {
                    let ty =
                        self.items(c).first().is_some_and(|&p| matches!(self.tree_of(p), T::TypeDef { .. }));
                    self.push(if i > 0 { ", " } else { "" });
                    self.push(if ty { "TypeParamClause(" } else { "TermParamClause(" });
                    self.list(c)?;
                    self.push(")");
                }
                self.push(if clauses.is_empty() { ", " } else { "), " });
                self.structure(tpt)?;
                self.rhs_structure(rhs)
            }
            T::TypeDef { name, rhs } => match self.h.tpd.class_template(t) {
                Some(tmpl) => {
                    self.push(&format!("ClassDef(\"{}\", ", self.name(name)));
                    self.structure(tmpl.constr)?;
                    self.push(", ");
                    self.list(tmpl.parents)?;
                    self.push(", ");
                    self.option(tmpl.self_)?;
                    self.push(", ");
                    self.list(tmpl.body)?;
                    self.push(")");
                    Ok(())
                }
                None => {
                    self.push(&format!("TypeDef(\"{}\", ", self.name(name)));
                    self.structure(rhs)?;
                    self.push(")");
                    Ok(())
                }
            },
            _ => Err(super::structure::STRUCTURE),
        }
    }

    /// `, ` the optional right-hand side `rhs`, `)`.
    fn rhs_structure(&mut self, rhs: Option<TNodeId>) -> Shown {
        self.push(", ");
        self.option(rhs)?;
        self.push(")");
        Ok(())
    }

    /// The symbol of tree `t` (dotty's `visitSymbol`).
    pub fn symbol_structure(&mut self, t: TNodeId) {
        let Some(s) = self.h.tpd.sym(self.r(t)) else { return self.push("NoSymbol()") };
        let what = match self.h.u.symbol(s).kind {
            Kind::Package => "IsPackageDefSymbol",
            Kind::Class => "IsClassDefSymbol",
            Kind::Def => "IsDefDefSymbol",
            Kind::Val | Kind::Param => "IsValDefSymbol",
            Kind::Type | Kind::TypeParam => "IsTypeDefSymbol",
        };
        let name = self.h.full_name(s);
        self.push(&format!("{what}(<{name}>)"));
    }
}
