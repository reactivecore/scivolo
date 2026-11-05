// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//! Whether the info of a type definition leads back to it other than through a legal F-bound
//! (dotty's `checkNonCyclic`, `CheckNonCyclicMap`).

use crate::symbols::Completion;
use crate::typer::Typer;
use syntax::ast::NodeId;
use tasty::{Kind, SymbolId, Type, TypeId};

/// How deep the check goes before it reports a cycle.
const MAX_DEPTH: u32 = 64;

/// A cycle found.
struct Cycle;

/// The state of a check of `sym`'s info.
struct NonCyclic {
    sym: SymbolId,
    locked: Vec<TypeId>,
    nested_ok: bool,
    cycle_ok: bool,
    variance: i8,
    depth: u32,
}

impl Typer<'_> {
    /// Checks that the bounds or alias `info` of type `sym` do not lead back to `sym` other than
    /// through a legal F-bound; else refuses it and gives `sym` an error info.
    pub fn check_non_cyclic(&mut self, sym: SymbolId, info: TypeId, tree: NodeId) {
        let mut c =
            NonCyclic { sym, locked: Vec::new(), nested_ok: false, cycle_ok: false, variance: 1, depth: 0 };
        let info = match self.u.opaque_alias(sym) {
            Some(alias) => self.u.intern_type(Type::Alias(alias)),
            None => info,
        };
        if self.check_info(&mut c, info).is_err() {
            self.unsupported(tree, "cyclic type reference");
            let error = self.u.intern_type(Type::Error);
            self.u.set_info(sym, error);
        }
    }

    fn check_info(&mut self, c: &mut NonCyclic, info: TypeId) -> Result<(), Cycle> {
        match *self.u.types().get(info) {
            Type::Lambda { result, .. } if matches!(self.u.types().get(result), Type::Bounds(..)) => {
                self.check_info(c, result)
            }
            Type::Lambda { result, .. } => {
                let alias = self.u.intern_type(Type::Alias(result));
                self.check_info(c, alias)
            }
            Type::Alias(a) if matches!(self.u.types().get(a), Type::Match { .. }) => {
                self.at_variance(c, 0, |t, c| t.check_upper(c, a))
            }
            Type::Bounds(lo, hi) => {
                self.at_variance(c, -1, |t, c| t.check_part(c, lo))?;
                self.check_upper(c, hi)
            }
            Type::Alias(a) => {
                self.at_variance(c, -1, |t, c| t.check_part(c, a))?;
                self.check_upper(c, a)
            }
            _ => Ok(()),
        }
    }

    fn at_variance(
        &mut self,
        c: &mut NonCyclic,
        variance: i8,
        f: impl FnOnce(&mut Self, &mut NonCyclic) -> Result<(), Cycle>,
    ) -> Result<(), Cycle> {
        let saved = std::mem::replace(&mut c.variance, variance);
        let r = f(self, c);
        c.variance = saved;
        r
    }

    fn check_upper(&mut self, c: &mut NonCyclic, t: TypeId) -> Result<(), Cycle> {
        let saved = std::mem::replace(&mut c.nested_ok, true);
        let r = self.check_part(c, t);
        c.nested_ok = saved;
        r
    }

    fn check_with(
        &mut self,
        c: &mut NonCyclic,
        t: TypeId,
        cycle_ok: bool,
        nested_ok: bool,
    ) -> Result<(), Cycle> {
        let saved = (c.cycle_ok, c.nested_ok);
        (c.cycle_ok, c.nested_ok) = (cycle_ok, nested_ok);
        let r = self.check_part(c, t);
        (c.cycle_ok, c.nested_ok) = saved;
        r
    }

    fn check_part(&mut self, c: &mut NonCyclic, t: TypeId) -> Result<(), Cycle> {
        if c.depth > MAX_DEPTH {
            return Err(Cycle);
        }
        c.depth += 1;
        let r = self.check_type(c, t);
        c.depth -= 1;
        r
    }

    fn check_type(&mut self, c: &mut NonCyclic, t: TypeId) -> Result<(), Cycle> {
        match self.u.types().get(t).clone() {
            Type::Sym(pre, s) if self.u.is_type_symbol(s) => self.check_type_ref(c, t, pre, s),
            Type::TypeRef(pre, _) => match self.u.resolve_ref(t) {
                Some(s) => self.check_type_ref(c, t, Some(pre), s),
                None => self.check_part(c, pre),
            },
            Type::Sym(pre, s) => {
                if matches!(self.syms.completions.get(&s), None | Some(Completion::Done)) {
                    let info = self.u.info(s);
                    self.check_part(c, info)?;
                }
                pre.map_or(Ok(()), |p| self.check_part(c, p))
            }
            Type::Refined(parent, _, info) => {
                self.check_part(c, parent)?;
                let nested = c.nested_ok;
                self.check_with(c, info, nested, nested)
            }
            Type::Match { bound, scrutinee, cases } => {
                self.check_part(c, bound)?;
                self.at_variance(c, 0, |t, c| t.check_part(c, scrutinee))?;
                cases.into_iter().try_for_each(|case| self.check_part(c, case))
            }
            Type::MatchCase(pat, body) => {
                let nested = c.nested_ok;
                self.check_with(c, pat, nested, nested)?;
                self.check_with(c, body, nested, nested)
            }
            Type::Applied(tycon, args) => {
                self.check_part(c, tycon)?;
                let tycon_sym = match *self.u.types().get(tycon) {
                    Type::Sym(_, s) => Some(s),
                    Type::TypeRef(..) => self.u.resolve_ref(tycon),
                    _ => None,
                };
                let signs = tycon_sym.map(|s| self.u.variances(s));
                for (i, arg) in args.into_iter().enumerate() {
                    let sign = signs.as_ref().and_then(|v| v.get(i)).copied().unwrap_or(0);
                    let nested = c.nested_ok || c.variance != 0 && c.variance != sign;
                    let (cycle_ok, variance) = (c.nested_ok, c.variance * sign);
                    self.at_variance(c, variance, |t, c| t.check_with(c, arg, cycle_ok, nested))?;
                }
                Ok(())
            }
            _ => {
                let mut parts = Vec::new();
                self.u.map_type(t, &mut |_, part| {
                    if part == t {
                        return None;
                    }
                    parts.push(part);
                    Some(part)
                });
                parts.into_iter().try_for_each(|p| self.check_part(c, p))
            }
        }
    }

    fn check_type_ref(
        &mut self,
        c: &mut NonCyclic,
        t: TypeId,
        pre: Option<TypeId>,
        s: SymbolId,
    ) -> Result<(), Cycle> {
        let r = self.check_interesting_ref(c, t, pre, s);
        if r.is_err() && c.cycle_ok { Ok(()) } else { r }
    }

    fn check_interesting_ref(
        &mut self,
        c: &mut NonCyclic,
        t: TypeId,
        pre: Option<TypeId>,
        s: SymbolId,
    ) -> Result<(), Cycle> {
        if !self.is_interesting(c.sym, pre) {
            return Ok(());
        }
        if let Some(p) = pre {
            let variance = c.variance.max(0);
            self.at_variance(c, variance, |t, c| t.check_with(c, p, false, false))?;
        }
        if c.locked.contains(&t) || s == c.sym {
            return Err(Cycle);
        }
        if self.u.symbol(s).kind == Kind::Class {
            return Ok(());
        }
        c.locked.push(t);
        self.complete(s);
        let info = match self.u.opaque_alias(s) {
            Some(alias) => self.u.intern_type(Type::Alias(alias)),
            None => self.u.info(s),
        };
        let r = self.check_info(c, info);
        c.locked.pop();
        r
    }

    /// Whether a reference with prefix `pre` may lead back to `sym` (dotty's `isInteresting`).
    fn is_interesting(&mut self, sym: SymbolId, pre: Option<TypeId>) -> bool {
        let Some(pre) = pre else { return true };
        let owner = self.u.symbol(sym).owner;
        match self.u.types().get(pre).clone() {
            Type::NoPrefix => true,
            Type::This(cls) => {
                let (Some(cls), Some(owner)) = (self.u.class_symbol(cls), owner) else { return false };
                self.u.is_class(owner)
                    && (crate::sealed::is_contained_in(self.u, cls, owner)
                        || crate::sealed::is_contained_in(self.u, owner, cls))
            }
            Type::Sym(p, s) => {
                let private = self.u.symbol(sym).flags.has(tasty::tags::PRIVATE);
                let widened = self.u.widen(pre);
                let derives = match (self.u.class_symbol(widened), owner) {
                    (Some(c), Some(o)) => self.u.derives_from(c, o),
                    _ => false,
                };
                let module = self.u.module_class(s).unwrap_or(s);
                !private && derives
                    || if self.u.is_static_symbol(module) {
                        self.syms.is_script(module)
                    } else {
                        self.is_interesting(sym, p)
                    }
            }
            Type::TermRef(p, _) => self.is_interesting(sym, Some(p)),
            Type::Super(this, _) => self.is_interesting(sym, Some(this)),
            Type::And(a, b) => self.is_interesting(sym, Some(a)) || self.is_interesting(sym, Some(b)),
            Type::Or(a, b, _) => self.is_interesting(sym, Some(a)) && self.is_interesting(sym, Some(b)),
            Type::Refined(..) | Type::Rec(..) | Type::Applied(..) => true,
            Type::Annotated(p, _) => self.is_interesting(sym, Some(p)),
            _ => false,
        }
    }
}
