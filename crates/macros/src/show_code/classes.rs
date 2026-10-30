// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Class definitions of `Printer.TreeCode`.

use super::{Code, REFUSED, Shown};
use tasty::tags::{
    CASE, FIELDaccessor, FINAL, IMPLICIT, LAZY, OBJECT, PARAMsetter, SEALED, SYNTHETIC, TRAIT,
};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree as T};

impl Code<'_> {
    /// Whether `s` has dotty's `Param` flag.
    pub fn is_param(&self, s: SymbolId) -> bool {
        matches!(self.h.u.symbol(s).kind, Kind::Param | Kind::TypeParam)
    }

    pub fn class_def(&mut self, t: TNodeId) -> Shown {
        let template = self.h.tpd.class_template(t).ok_or(REFUSED)?;
        let T::TypeDef { name, .. } = self.tree_of(t) else { return Err(REFUSED) };
        let (cls, name) = (self.h.tpd.sym(t).ok_or(REFUSED)?, self.name(name));
        let s = Some(cls);
        self.def_annotations(cls)?;
        for (tag, word) in [(IMPLICIT, "implicit "), (SEALED, "sealed "), (FINAL, "final "), (CASE, "case ")] {
            if self.has(s, tag) && !(tag == FINAL && self.has(s, OBJECT)) {
                self.keyword(word);
            }
        }
        if name == "package$" {
            self.keyword("package object ");
            let owner = self.h.u.symbol(cls).owner.map(|o| self.h.u.symbol(o).name.clone()).unwrap_or_default();
            self.highlight_type(owner.strip_suffix('$').unwrap_or(&owner));
        } else if self.has(s, OBJECT) {
            self.keyword("object ");
            self.highlight_type(name.strip_suffix('$').unwrap_or(&name));
        } else {
            let abstract_ = self.has(s, tasty::tags::ABSTRACT);
            self.keyword(if self.has(s, TRAIT) { "trait " } else if abstract_ { "abstract class " } else { "class " });
            self.highlight_type(&name);
        }
        let stats = self.items(template.body);
        if !self.has(s, OBJECT) {
            let T::DefDef { paramss, .. } = self.tree_of(template.constr) else { return Err(REFUSED) };
            for clause in self.h.tpd.clause_items(paramss).to_vec() {
                let params = self.items(clause);
                match params.first().map(|&p| self.tree_of(p)) {
                    Some(T::TypeDef { .. }) => {
                        let own: Vec<_> = stats.iter().copied().filter(|&d| self.is_type_param_def(d)).collect();
                        self.class_type_params(&own, &params)?;
                    }
                    _ => self.method_params(&params)?,
                }
            }
        }
        let parents: Vec<_> =
            self.items(template.parents).into_iter().filter(|&p| !self.is_elided_parent(p)).collect();
        if !parents.is_empty() {
            self.keyword(" extends ");
        }
        for (i, &p) in parents.iter().enumerate() {
            if i > 0 {
                self.keyword(" with ");
            }
            self.parent(p, false)?;
        }
        let stats: Vec<_> = stats.into_iter().filter(|&d| self.keep_stat(d)).collect();
        let self_ = template.self_.map(|v| self.tree_of(v));
        match self_ {
            Some(T::ValDef { tpt, .. }) if matches!(self.tree_of(tpt), T::SingletonTypeTree { .. }) => {
                if !stats.is_empty() {
                    self.class_body(&stats, None)?;
                }
            }
            Some(T::ValDef { name, tpt, .. }) => self.class_body(&stats, Some((self.name(name), tpt)))?,
            _ if !stats.is_empty() => self.class_body(&stats, None)?,
            _ => {}
        }
        Ok(())
    }

    fn is_type_param_def(&self, d: TNodeId) -> bool {
        matches!(self.tree_of(d), T::TypeDef { .. })
            && self.h.tpd.sym(d).is_some_and(|s| self.h.u.symbol(s).kind == Kind::TypeParam)
    }

    /// The class's type parameters `own` with the bounds of the constructor's `params`.
    fn class_type_params(&mut self, own: &[TNodeId], params: &[TNodeId]) -> Shown {
        if own.is_empty() {
            return Ok(());
        }
        self.push("[");
        for (i, (&d, &p)) in own.iter().zip(params).enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.variance(self.h.tpd.sym(d));
            let T::TypeDef { name, rhs } = self.tree_of(p) else { return Err(REFUSED) };
            self.push(&self.name(name));
            self.type_def_rhs(rhs)?;
        }
        self.push("]");
        Ok(())
    }

    /// Whether parent `p` is a call of `java.lang.Object`'s constructor or `_root_.scala.Product`
    /// or `_root_.scala.Serializable`.
    fn is_elided_parent(&mut self, p: TNodeId) -> bool {
        match self.tree_of(p) {
            T::Apply { fun, .. } => {
                let T::Select { qualifier, .. } = self.tree_of(self.r(fun)) else { return false };
                let T::New { tpt } = self.tree_of(self.r(qualifier)) else { return false };
                let tpe = self.h.tpd.tpe(self.r(tpt));
                self.h.u.class_symbol(tpe).is_some_and(|c| self.h.u.full_name(c) == "java.lang.Object")
            }
            T::Select { qualifier, name } if matches!(&*self.name(name), "Product" | "Serializable") => {
                matches!(self.tree_of(qualifier), T::Select { qualifier: root, name }
                    if self.name(name) == "scala"
                        && matches!(self.tree_of(root), T::Ident { name } if self.name(name) == "_root_"))
            }
            _ => false,
        }
    }

    fn parent(&mut self, p: TNodeId, need_empty_parens: bool) -> Shown {
        let p = self.r(p);
        if self.is_type_tree(p) {
            return self.type_tree(p);
        }
        match self.tree_of(p) {
            T::TypeApply { fun, .. } => self.parent(fun, false),
            T::Apply { fun, args } => {
                let nested = matches!(self.tree_of(self.r(fun)), T::Apply { .. });
                self.parent(fun, nested)?;
                let args = self.items(args);
                if !args.is_empty() || need_empty_parens {
                    self.push("(");
                    self.trees(&args, ", ")?;
                    self.push(")");
                }
                Ok(())
            }
            T::Select { qualifier, .. } if matches!(self.tree_of(self.r(qualifier)), T::New { .. }) => {
                self.ty(self.h.tpd.tpe(self.r(qualifier)))
            }
            _ => Err(REFUSED),
        }
    }

    /// dotty's `keepDefinition` for definitions; imports and terms are kept.
    fn keep_stat(&self, d: TNodeId) -> bool {
        if !matches!(self.tree_of(d), T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. }) {
            return true;
        }
        let Some(s) = self.h.tpd.sym(d) else { return true };
        let f = self.h.u.symbol(s).flags;
        let owner = self.h.u.symbol(s).owner;
        let owner_has = |tag| owner.is_some_and(|o| self.h.u.symbol(o).flags.has(tag));
        let name = self.h.u.symbol(s).name.as_str();
        let numbered = |n: &str| n.strip_prefix('_').is_some_and(|d| !d.starts_with('0') && d.parse::<u32>().is_ok());
        let undecompilable = f.has(SYNTHETIC)
            && matches!(self.tree_of(d), T::DefDef { .. })
            && ((matches!(name, "apply" | "unapply" | "writeReplace") && owner_has(OBJECT))
                || (owner_has(CASE)
                    && (name == "copy"
                        || name.strip_prefix("copy$default$").is_some_and(|d| numbered(&format!("_{d}")))
                        || numbered(name)
                        || name == "productElementName")));
        !self.is_param(s)
            && !f.has(PARAMsetter)
            && !f.has(FIELDaccessor)
            && !undecompilable
            && !(f.has(LAZY) && f.has(OBJECT))
    }

    fn class_body(&mut self, stats: &[TNodeId], self_: Option<(String, TNodeId)>) -> Shown {
        self.push(" {");
        self.indent += 1;
        if let Some((name, tpt)) = self_ {
            self.indent += 1;
            self.push(" ");
            self.definition_name(if name == "_" { "this" } else { &name });
            self.push(": ");
            self.type_tree(tpt)?;
            self.push(" =>");
            self.indent -= 1;
        }
        self.line_break();
        for (i, &s) in stats.iter().enumerate() {
            if i > 0 {
                self.line_break();
            }
            self.tree(s)?;
        }
        self.indent -= 1;
        self.line_break();
        self.push("}");
        Ok(())
    }
}
