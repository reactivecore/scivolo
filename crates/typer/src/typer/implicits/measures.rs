// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The size of a type and the symbols it is built from, which the divergence check compares
//! (dotty's `typeSize`, `coveringSet`, their `TypeSizeAccumulator` and `CoveringSetAccumulator`).

use super::super::Typer;
use tasty::{SymbolId, Type, TypeId, TypeVars, VarId};

const OUTSIDE: &str = "divergence check of a type outside the subset";

/// dotty's `typeSize` and `coveringSet`.
#[derive(Clone, PartialEq)]
pub struct Measure {
    pub size: u32,
    pub covering: Vec<SymbolId>,
}

impl Typer<'_> {
    pub fn measure(&mut self, t: TypeId) -> Result<Measure, &'static str> {
        let t = self.zonk(t);
        let size = self.type_size(0, t, &mut Vec::new())?;
        let mut covering = Vec::new();
        self.cover(&mut covering, t, &mut Vec::new())?;
        covering.sort_unstable_by_key(|s| s.0);
        covering.dedup();
        Ok(Measure { size, covering })
    }

    /// The parts of `t` a size or covering set folds over: its prefix, arguments and operands.
    fn parts(&mut self, t: TypeId) -> Result<Vec<TypeId>, &'static str> {
        Ok(match self.u.types().get(t).clone() {
            Type::Sym(p, _) => p.into_iter().collect(),
            Type::TypeRef(p, _) | Type::TermRef(p, _) | Type::This(p) => vec![p],
            Type::Applied(c, args) => [vec![c], args].concat(),
            Type::Refined(p, _, i) | Type::Bounds(p, i) | Type::And(p, i) | Type::Or(p, i, _) => vec![p, i],
            Type::ByName(r) | Type::Annotated(r, _) => vec![r],
            Type::NoPrefix | Type::Package(_) | Type::PackageName(_) | Type::Str(_) | Type::Constant(..) => {
                Vec::new()
            }
            _ => return Err(OUTSIDE),
        })
    }

    /// `t` dealiased if it is a reference to an alias, `None` if not.
    fn alias_of(&mut self, t: TypeId) -> Result<Option<TypeId>, &'static str> {
        if !matches!(self.u.types().get(t), Type::Sym(..) | Type::TypeRef(..)) {
            return Ok(None);
        }
        let d = self.u.dealias(t);
        match self.u.types().get(d) {
            _ if d == t => Ok(None),
            Type::Lambda { .. } | Type::Match { .. } => Err(OUTSIDE),
            _ => Ok(Some(d)),
        }
    }

    fn type_size(&mut self, n: u32, t: TypeId, seen: &mut Vec<VarId>) -> Result<u32, &'static str> {
        if let Some(d) = self.alias_of(t)? {
            return self.type_size(n, d, seen);
        }
        match self.u.types().get(t).clone() {
            Type::Var(v) if seen.contains(&v) => Ok(n),
            Type::Var(v) => {
                seen.push(v);
                let (lo, hi) = self.constraint.bounds(v);
                let (lo, hi) = (self.zonk(lo), self.zonk(hi));
                Ok(self.type_size(n, lo, seen)?.max(self.type_size(n, hi, seen)?))
            }
            Type::Applied(c, args) if self.tuple_arity(c) == Some(args.len()) => {
                let mut n = n + args.len() as u32;
                for a in args {
                    n = self.type_size(n, a, seen)?;
                }
                Ok(n)
            }
            Type::Applied(c, _) if self.alias_of(c)?.is_some() => Err(OUTSIDE),
            other => {
                let mut n = n + matches!(other, Type::Applied(..) | Type::Refined(..)) as u32;
                for p in self.parts(t)? {
                    n = self.type_size(n, p, seen)?;
                }
                Ok(n)
            }
        }
    }

    /// `N` if `c` is the class `scala.TupleN`.
    fn tuple_arity(&mut self, c: TypeId) -> Option<usize> {
        let class = self.u.class_symbol(c)?;
        let name = self.u.full_name(class);
        name.strip_prefix("scala.Tuple")?.parse().ok().filter(|&n| (2..=22).contains(&n))
    }

    /// Adds the symbols `t` is built from to `cs`, those of the prefixes of references to symbols
    /// that are not static included (dotty's `StopAt.Static`).
    fn cover(
        &mut self,
        cs: &mut Vec<SymbolId>,
        t: TypeId,
        seen: &mut Vec<TypeId>,
    ) -> Result<(), &'static str> {
        if seen.contains(&t) {
            return Ok(());
        }
        seen.push(t);
        if let Some(d) = self.alias_of(t)? {
            return self.cover(cs, d, seen);
        }
        let b = self.u.builtins();
        let (fold, own): (Vec<TypeId>, Option<SymbolId>) = match self.u.types().get(t).clone() {
            Type::Var(v) => {
                let (lo, hi) = self.constraint.bounds(v);
                (vec![lo, hi], None)
            }
            Type::Applied(c, args) => ([vec![c], args].concat(), self.type_symbol(c)),
            Type::Refined(p, _, i) => (vec![p, i], self.type_symbol(p)),
            Type::Sym(..) | Type::TypeRef(..) | Type::TermRef(..) => {
                let (p, s) = match *self.u.types().get(t) {
                    Type::Sym(p, s) => (p, s),
                    Type::TypeRef(p, _) | Type::TermRef(p, _) => {
                        (Some(p), self.u.resolve_ref(t).ok_or(OUTSIDE)?)
                    }
                    _ => return Err(OUTSIDE),
                };
                if s == b.any || s == b.nothing {
                    return Ok(());
                }
                let own =
                    (self.u.symbol(s).kind == tasty::Kind::Class || !self.u.is_type_symbol(s)).then_some(s);
                (p.filter(|_| !self.u.is_static_symbol(s)).into_iter().collect(), own)
            }
            Type::This(c) => (Vec::new(), self.type_symbol(c)),
            Type::Package(p) => (Vec::new(), Some(p)),
            _ => (self.parts(t)?, None),
        };
        if let Some(s) = own {
            if self.u.symbol(s).kind == tasty::Kind::Param {
                return Err(OUTSIDE);
            }
            cs.push(s);
        }
        for p in fold {
            self.cover(cs, p, seen)?;
        }
        Ok(())
    }

    /// The symbol a reference or applied type refers to.
    fn type_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        match *self.u.types().get(t) {
            Type::Sym(_, s) => Some(s),
            Type::Applied(c, _) => self.type_symbol(c),
            _ => self.u.class_symbol(t),
        }
    }
}
