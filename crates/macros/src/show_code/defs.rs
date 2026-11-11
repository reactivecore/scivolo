// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Definitions, parameters and patterns of `Printer.TreeCode`.

use super::{Code, REFUSED, Shown};
use names::nme;
use tasty::SymbolId;
use tasty::tags::{
    CASEaccessor, ERASED, GIVEN, IMPLICIT, LAZY, LOCAL, MUTABLE, OBJECT, OVERRIDE, PARAMsetter,
};
use typer::tpd::{TNodeId, Tree as T};

impl Code<'_> {
    pub fn has(&self, s: Option<SymbolId>, tag: u8) -> bool {
        s.is_some_and(|s| self.h.u.symbol(s).flags.has(tag))
    }

    /// `ValDef`, `DefDef` and the type definitions without a class.
    pub fn definition(&mut self, t: TNodeId) -> Shown {
        let s = self.h.tpd.sym(t);
        if let Some(s) = s {
            self.def_annotations(s)?;
        }
        match self.tree_of(t) {
            T::ValDef { name, tpt, rhs } => {
                self.modifiers(s, false);
                if self.has(s, LAZY) || self.has(s, OBJECT) {
                    self.keyword("lazy ");
                }
                self.keyword(if self.has(s, MUTABLE) { "var " } else { "val " });
                let name = self.def_name(s, &self.name(name));
                self.definition_name(&name);
                self.push(": ");
                self.type_tree(tpt)?;
                if let Some(rhs) = rhs {
                    self.push(" = ");
                    self.tree(rhs)?;
                }
            }
            T::DefDef { name, paramss, tpt, rhs } => {
                let constructor = self.name(name) == nme::CONSTRUCTOR;
                self.modifiers(s, true);
                let name = if constructor { "this".to_owned() } else { self.def_name(s, &self.name(name)) };
                self.keyword("def ");
                self.definition_name(&name);
                for clause in self.h.tpd.clause_items(paramss).to_vec() {
                    let params = self.items(clause);
                    match params.first().map(|&p| self.tree_of(p)) {
                        Some(T::TypeDef { .. }) => self.type_params(&params)?,
                        _ => self.method_params(&params)?,
                    }
                }
                if !constructor {
                    self.push(": ");
                    self.type_tree(tpt)?;
                }
                if let Some(rhs) = rhs {
                    self.push(" = ");
                    self.tree(rhs)?;
                }
            }
            T::TypeDef { name, rhs } if !matches!(self.tree_of(self.r(rhs)), T::LambdaTypeTree { .. }) => {
                self.keyword("type ");
                self.variance(s);
                self.push(&self.name(name));
                self.type_def_rhs(rhs)?;
            }
            _ => return Err(REFUSED),
        }
        Ok(())
    }

    fn def_name(&mut self, s: Option<SymbolId>, name: &str) -> String {
        match s.filter(|&s| self.h.u.is_local(s)) {
            Some(s) => self.spliced_name(s),
            None => name.to_owned(),
        }
    }

    fn type_params(&mut self, params: &[TNodeId]) -> Shown {
        self.push("[");
        for (i, &p) in params.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            let T::TypeDef { name, rhs } = self.tree_of(p) else { return Err(REFUSED) };
            self.variance(self.h.tpd.sym(p));
            self.push(&self.name(name));
            self.type_def_rhs(rhs)?;
        }
        self.push("]");
        Ok(())
    }

    pub fn method_params(&mut self, params: &[TNodeId]) -> Shown {
        let first = params.first().and_then(|&p| self.h.tpd.sym(p));
        self.push("(");
        if self.has(first, GIVEN) {
            self.push("using ");
        } else if self.has(first, IMPLICIT) {
            self.push("implicit ");
        }
        self.params(params)?;
        self.push(")");
        Ok(())
    }

    pub fn lambda_params(&mut self, params: &[TNodeId]) -> Shown {
        let first = params.first().and_then(|&p| self.h.tpd.sym(p));
        self.push("(");
        if self.has(first, IMPLICIT) && !self.has(first, GIVEN) {
            self.push("implicit ");
        }
        self.params(params)?;
        self.push(")");
        Ok(())
    }

    fn params(&mut self, params: &[TNodeId]) -> Shown {
        for (i, &p) in params.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            let T::ValDef { name, tpt, .. } = self.tree_of(p) else { return Err(REFUSED) };
            let s = self.h.tpd.sym(p);
            let name = self.def_name(s, &self.name(name));
            if self.has(s, ERASED) {
                self.push("erased ");
            }
            let owner = s.and_then(|s| self.h.u.symbol(s).owner);
            if let Some(ctor) = owner.filter(|&o| self.h.u.symbol(o).name == nme::CONSTRUCTOR) {
                let cls = self.h.u.symbol(ctor).owner.ok_or(REFUSED)?;
                let decls = self.h.u.symbol(cls).decls.clone();
                let accessor = decls.into_iter().find(|&d| {
                    let d = self.h.u.symbol(d);
                    d.kind.is_term() && d.flags.has(PARAMsetter) && d.name == name
                });
                if let Some(a) = accessor.filter(|&a| !self.has(Some(a), LOCAL)) {
                    let mut prefixed = self.has(Some(a), OVERRIDE);
                    if prefixed {
                        self.push("override ");
                    }
                    prefixed |= self.protected_or_private(Some(a));
                    if self.has(Some(a), MUTABLE) {
                        self.definition_name("var ");
                    } else if prefixed || !self.has(Some(a), CASEaccessor) {
                        self.definition_name("val ");
                    }
                }
            }
            self.definition_name(&name);
            self.push(": ");
            self.type_tree(tpt)?;
        }
        Ok(())
    }
}
