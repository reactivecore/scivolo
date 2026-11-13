// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala

use super::Cmp;
use crate::types::{Type, TypeId};
use crate::{Kind, SymbolId, Universe};

/// How many aliases, bounds and instances `class_symbols` follows one inside another.
const MAX_DEPTH: u32 = 64;

impl Universe {
    /// Finds the least class `t` is below (dotty's `classSymbol`): of an intersection the part's
    /// class deriving from the other's, of a union its join's.
    pub fn least_class(&mut self, t: TypeId) -> Option<SymbolId> {
        self.least_class_within(t, MAX_DEPTH)
    }

    fn least_class_within(&mut self, t: TypeId, depth: u32) -> Option<SymbolId> {
        let depth = depth.checked_sub(1)?;
        let t = self.dealias(t);
        match *self.ty(t) {
            Type::And(a, b) => {
                let (a, b) = (self.least_class_within(a, depth)?, self.least_class_within(b, depth)?);
                if self.derives_from(a, b) { Some(a) } else { self.derives_from(b, a).then_some(b) }
            }
            Type::Or(..) => self.join(t).filter(|&j| j != t).and_then(|j| self.least_class_within(j, depth)),
            _ => match self.sym_of(t) {
                Some(s) if matches!(self.symbol(s).kind, Kind::Type | Kind::TypeParam) => {
                    let info = self.info(s);
                    let hi = self.bounds(info).map_or(info, |(_, hi)| hi);
                    self.least_class_within(hi, depth)
                }
                _ => self.class_symbol(t),
            },
        }
    }
}

impl Cmp<'_> {
    /// The least set of classes `t` is below (dotty's `classSymbols`).
    pub fn class_symbols(&mut self, t: TypeId) -> Vec<SymbolId> {
        self.class_symbols_within(t, MAX_DEPTH)
    }

    fn class_symbols_within(&mut self, t: TypeId, depth: u32) -> Vec<SymbolId> {
        let Some(depth) = depth.checked_sub(1) else { return Vec::new() };
        let sym = match *self.u.ty(t) {
            Type::And(x, y) => {
                let mut cs = self.class_symbols_within(x, depth);
                for c in self.class_symbols_within(y, depth) {
                    if !cs.contains(&c) {
                        cs.push(c);
                    }
                }
                return cs;
            }
            Type::Or(x, y, _) => {
                let ys = self.class_symbols_within(y, depth);
                let xs = self.class_symbols_within(x, depth);
                return xs.into_iter().filter(|c| ys.contains(c)).collect();
            }
            Type::Var(v) => {
                let next = self.vars.instance(v).unwrap_or_else(|| self.vars.bounds(v).1);
                return self.class_symbols_within(next, depth);
            }
            Type::Applied(inner, _)
            | Type::Annotated(inner, _)
            | Type::Flexible(inner)
            | Type::Alias(inner)
            | Type::Refined(inner, ..)
            | Type::Rec(_, inner)
            | Type::Bounds(_, inner)
            | Type::Lambda { result: inner, .. } => return self.class_symbols_within(inner, depth),
            Type::Sym(_, s) => s,
            Type::TypeRef(..) => {
                let Some(s) = self.u.resolve_ref(t) else { return Vec::new() };
                s
            }
            _ => return Vec::new(),
        };
        match self.u.symbol(sym).kind {
            Kind::Class => vec![sym],
            Kind::Type | Kind::TypeParam => {
                let info = self.u.info(sym);
                self.class_symbols_within(info, depth)
            }
            _ => Vec::new(),
        }
    }
}
