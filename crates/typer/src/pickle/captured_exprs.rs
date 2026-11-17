//! The types dotty's typer gives the expressions of a capture-checked file with the capture sets
//! ours drops, where the definitions an expression refers to tell them.

use super::TreePickler;
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::capture_types::has_captures;
use tasty::{Kind, Type, TypeId};

/// The most nested expressions a type is told through.
const MAX_DEPTH: usize = 64;

/// The type dotty's typer gives an expression, as far as capture sets go.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Captured {
    /// The type ours gives, without capture sets.
    Ours,
    /// This type, with capture sets.
    With(TypeId),
}

/// `t` as a `Captured`.
pub fn captured(u: &mut tasty::Universe, t: TypeId) -> Captured {
    match has_captures(u, t) {
        true => Captured::With(t),
        false => Captured::Ours,
    }
}

impl TreePickler<'_> {
    /// Determines the type dotty's typer gives expression `e`, `None` if the definitions it refers
    /// to do not tell it: of a literal, `this` or a type ascription its type; of a reference or a
    /// call the info of its definition as seen from its qualifier, with inferred type arguments
    /// that the arguments of parameters of those types tell, a singleton widened (not told if a
    /// parameter's, or `this.type` seen from a path with capture sets); of a block its result's or the
    /// function type of the closure it ends in; of a conditional or a match the one of all its
    /// branches but those of type `Nothing`. Records the type of each inferred type argument told.
    pub fn captured_expr(&mut self, e: TNodeId) -> Option<Captured> {
        self.captured_in(e, MAX_DEPTH)
    }

    /// The type of expression `e`, told through at most `depth` nested expressions.
    pub fn captured_in(&mut self, e: TNodeId, depth: usize) -> Option<Captured> {
        let depth = depth.checked_sub(1)?;
        match *self.tpd.tree(e) {
            T::Literal { .. } | T::This { .. } => Some(Captured::Ours),
            T::Typed { tpt, .. } => self.captured_tpt(tpt),
            T::Block { expr, .. } => match *self.tpd.tree(expr) {
                T::Closure { tpt: None, .. } => self.captured_closure(e, depth),
                _ => self.captured_in(expr, depth),
            },
            T::If { thenp, elsep, .. } => self.captured_branches(&[thenp, elsep], depth),
            T::Match { cases, .. } => {
                let bodies: Vec<TNodeId> = (self.tpd.items(cases).iter())
                    .filter_map(|&c| match *self.tpd.tree(c) {
                        T::CaseDef { body, .. } => Some(body),
                        _ => None,
                    })
                    .collect();
                self.captured_branches(&bodies, depth)
            }
            T::Ident { .. } | T::Select { .. } | T::Apply { .. } | T::TypeApply { .. } => {
                self.captured_call(e, depth)
            }
            _ => None,
        }
    }

    /// The type of type tree `tpt`: an inferred one's as recorded.
    pub fn captured_tpt(&mut self, tpt: TNodeId) -> Option<Captured> {
        let tpe = self.tpd.get(tpt).tpe?;
        if !matches!(self.tpd.tree(tpt), T::TypeTree { inferred: true }) {
            return Some(captured(self.u, tpe));
        }
        if let Some(t) = self.tpd.captured_type(tpt).or_else(|| self.kept_captures.get(&tpt).copied()) {
            return Some(captured(self.u, t));
        }
        self.inferred.capture_free.contains(&tpt).then_some(Captured::Ours)
    }

    fn captured_branches(&mut self, branches: &[TNodeId], depth: usize) -> Option<Captured> {
        let mut found = None;
        for &b in branches.iter().filter(|&&b| self.tpd.get(b).tpe != Some(tasty::NOTHING)) {
            let c = self.captured_in(b, depth)?;
            if found.is_some_and(|f| f != c) {
                return None;
            }
            found = Some(c);
        }
        found
    }

    /// The type of reference or call `e`.
    fn captured_call(&mut self, e: TNodeId, depth: usize) -> Option<Captured> {
        let (mut fun, mut clauses, mut targs) = (e, Vec::new(), None);
        loop {
            match *self.tpd.tree(fun) {
                T::Apply { fun: f, args } if targs.is_none() => {
                    clauses.insert(0, args);
                    fun = f;
                }
                T::TypeApply { fun: f, args } if targs.is_none() => (fun, targs) = (f, Some(args)),
                _ => break,
            }
        }
        let s = self.tpd.sym(fun)?;
        let mut info = self.ref_info(fun, s, depth)?;
        if let Some(targs) = targs {
            info = self.instantiated((info, s), targs, &clauses, depth)?;
        }
        for _ in &clauses {
            info = self.u.method(info)?.result;
        }
        let mut info = self.u.by_name(info).unwrap_or(info);
        let param = |u: &tasty::Universe, s: tasty::SymbolId| u.symbol(s).kind == Kind::Param;
        match *self.u.ty(info) {
            Type::Param(..) | Type::Local(..) => return None,
            Type::Sym(_, p) if param(self.u, p) => return None,
            _ => {}
        }
        if is_path(self.u, info) {
            info = self.u.widen(info);
        }
        if self.u.method(info).is_some() || self.u.poly(info).is_some() {
            return None;
        }
        Some(captured(self.u, info))
    }

    /// The info of the definition `s` that reference `fun` refers to, as seen from its qualifier:
    /// of a definition of the file with an inferred type, the type told for it.
    fn ref_info(&mut self, fun: TNodeId, s: tasty::SymbolId, depth: usize) -> Option<TypeId> {
        if !matches!(self.u.symbol(s).kind, Kind::Val | Kind::Param | Kind::Def) {
            return self.tpd.get(fun).tpe;
        }
        if let Some(&t) = self.inferred.kept_infos.get(&s) {
            return Some(t);
        }
        if self.inferred.risks.as_ref().is_some_and(|r| r.is_unknown(s)) {
            return None;
        }
        match *self.tpd.tree(fun) {
            T::Ident { .. } => Some(self.u.info(s)),
            T::Select { qualifier, .. } => {
                let declared = self.u.info(s);
                let declared = final_result(self.u, declared);
                let prefix = match self.captured_in(qualifier, depth)? {
                    Captured::With(_) if is_path(self.u, declared) => return None,
                    Captured::With(t) => strip_annotations(self.u, t),
                    Captured::Ours => self.tpd.get(qualifier).tpe?,
                };
                Some(match *self.u.ty(prefix) {
                    Type::Package(_) | Type::PackageName(_) => self.u.info(s),
                    _ => self.u.member_info(prefix, s),
                })
            }
            _ => None,
        }
    }
}

/// Whether `t` is a singleton type other than a constant, or a reference to a parameter.
fn is_path(u: &tasty::Universe, t: TypeId) -> bool {
    match *u.ty(t) {
        Type::TermRef(..) | Type::This(_) | Type::RecThis(_) | Type::Skolem(..) => true,
        Type::Param(..) | Type::Local(..) => true,
        Type::Sym(_, s) => u.symbol(s).kind.is_term(),
        _ => false,
    }
}

/// The result of method or by-name type `t` after all its parameter clauses.
fn final_result(u: &tasty::Universe, mut t: TypeId) -> TypeId {
    loop {
        t = match *u.ty(t) {
            Type::Method { result, .. } | Type::Poly { result, .. } | Type::ByName(result) => result,
            _ => return t,
        };
    }
}

/// Strips the annotations around `t`.
pub fn strip_annotations(u: &tasty::Universe, mut t: TypeId) -> TypeId {
    while let Some((inner, _)) = u.annotated(t) {
        t = inner;
    }
    t
}
