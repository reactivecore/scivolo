// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Type lambdas, method and poly types and their parameters of `Printer.TypeReprCode`.

use super::{Highlight, REFUSED, Shown, SourcePrinter};
use crate::types::{Binder, Type, TypeId};

impl SourcePrinter<'_> {
    /// Binding type or parameter reference `t`.
    pub fn binding(&mut self, t: Type) -> Shown {
        match t {
            Type::Lambda { binder, params, result } => self.bind(binder, &params, |p| {
                p.out.push('[');
                p.params(&params)?;
                p.out.push(']');
                p.paint(Highlight::Type, " =>> ");
                p.print(result)
            }),
            Type::Method { binder, params, result, .. } => self.bind(binder, &params, |p| {
                p.out.push('(');
                p.params(&params)?;
                p.out.push(')');
                p.print(result)
            }),
            Type::Poly { binder, params, result } => self.bind(binder, &params, |p| {
                p.out.push('[');
                for (i, &(ref name, info)) in params.iter().enumerate() {
                    p.out.push_str(if i > 0 { ", " } else { "" });
                    p.out.push_str(&format!("{name} "));
                    p.print(info)?;
                }
                p.out.push(']');
                p.print(result)
            }),
            Type::Param(b, i) => {
                let names = self.binders.iter().rev().find(|(binder, _)| *binder == b);
                let name = names.and_then(|(_, n)| n.get(i as usize)).ok_or(REFUSED)?;
                self.out.push_str(&name.clone());
                Ok(())
            }
            _ => Err(REFUSED),
        }
    }

    /// Runs `f` inside the binder `binder` of parameters `params`.
    pub fn bind(
        &mut self,
        binder: Binder,
        params: &[(String, TypeId)],
        f: impl FnOnce(&mut Self) -> Shown,
    ) -> Shown {
        self.binders.push((binder, params.iter().map(|(n, _)| n.clone()).collect()));
        let shown = f(self);
        self.binders.pop();
        shown
    }

    /// Parameters `params` as `printMethodicTypeParams` shows them.
    pub fn params(&mut self, params: &[(String, TypeId)]) -> Shown {
        for (i, &(ref name, info)) in params.iter().enumerate() {
            self.out.push_str(if i > 0 { ", " } else { "" });
            self.out.push_str(name);
            match *self.u.ty(info) {
                Type::Bounds(lo, hi) => self.bounds(lo, hi)?,
                Type::Alias(a) => self.bounds(a, a)?,
                _ => {
                    self.out.push_str(": ");
                    self.print(info)?;
                }
            }
        }
        Ok(())
    }
}
