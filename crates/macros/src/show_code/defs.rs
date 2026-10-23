// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Definitions, parameters and patterns of `Printer.TreeCode`.

use super::{Code, REFUSED, Shown};
use tasty::SymbolId;
use tasty::tags::{
    ERASED, FINAL, GIVEN, IMPLICIT, INLINE, LAZY, LOCAL, MUTABLE, OBJECT, OVERRIDE, PRIVATE, PROTECTED,
};
use typer::tpd::{TNodeId, Tree as T};

impl Code<'_> {
    fn has(&self, s: Option<SymbolId>, tag: u8) -> bool {
        s.is_some_and(|s| self.h.u.symbol(s).flags.has(tag))
    }

    /// `ValDef`, `DefDef` and the type definitions without a class.
    pub fn definition(&mut self, t: TNodeId) -> Shown {
        let s = self.h.tpd.sym(t);
        match self.tree_of(t) {
            T::ValDef { name, tpt, rhs } => {
                self.modifiers(s, false);
                if self.has(s, LAZY) {
                    self.push("lazy ");
                }
                self.push(if self.has(s, MUTABLE) { "var " } else { "val " });
                let name = self.def_name(s, &self.name(name));
                self.push(&format!("{name}: "));
                self.type_tree(tpt)?;
                if let Some(rhs) = rhs {
                    self.push(" = ");
                    self.tree(rhs)?;
                }
            }
            T::DefDef { name, paramss, tpt, rhs } => {
                let constructor = self.name(name) == "<init>";
                self.modifiers(s, true);
                let name = if constructor { "this".to_owned() } else { self.def_name(s, &self.name(name)) };
                self.push(&format!("def {name}"));
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
            _ => return Err(REFUSED),
        }
        Ok(())
    }

    fn def_name(&mut self, s: Option<SymbolId>, name: &str) -> String {
        match s.filter(|&s| self.is_local(s)) {
            Some(s) => self.spliced_name(s),
            None => name.to_owned(),
        }
    }

    /// The modifiers of definition `s` before `val`, `var` or `def`.
    fn modifiers(&mut self, s: Option<SymbolId>, def: bool) {
        if self.has(s, IMPLICIT) {
            self.push("implicit ");
        }
        if def && self.has(s, INLINE) {
            self.push("inline ");
        }
        if self.has(s, OVERRIDE) {
            self.push("override ");
        }
        if self.has(s, FINAL) && !self.has(s, OBJECT) {
            self.push("final ");
        }
        let access = if self.has(s, PROTECTED) {
            "protected"
        } else if self.has(s, PRIVATE) {
            "private"
        } else {
            return;
        };
        self.push(access);
        if self.has(s, LOCAL) {
            self.push("[this]");
        }
        self.push(" ");
    }

    fn type_params(&mut self, params: &[TNodeId]) -> Shown {
        self.push("[");
        for (i, &p) in params.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            let T::TypeDef { name, rhs } = self.tree_of(p) else { return Err(REFUSED) };
            let s = self.h.tpd.sym(p);
            if self.has(s, tasty::tags::COVARIANT) {
                self.push("+");
            } else if self.has(s, tasty::tags::CONTRAVARIANT) {
                self.push("-");
            }
            self.push(&self.name(name));
            let T::TypeBoundsTree { lo, hi, alias: None } = self.tree_of(rhs) else { return Err(REFUSED) };
            self.bounds_tree(lo, hi)?;
        }
        self.push("]");
        Ok(())
    }

    fn method_params(&mut self, params: &[TNodeId]) -> Shown {
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
            let constructor =
                s.and_then(|s| self.h.u.symbol(s).owner).is_some_and(|o| self.h.u.symbol(o).name == "<init>");
            if constructor {
                return Err(REFUSED);
            }
            if self.has(s, ERASED) {
                self.push("erased ");
            }
            let name = self.def_name(s, &self.name(name));
            self.push(&format!("{name}: "));
            self.type_tree(tpt)?;
        }
        Ok(())
    }
}
