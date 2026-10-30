// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! Arguments the call's type refers to by skolems: their proxies typed by those skolems and the
//! expansion cast to the call's type (dotty's `callValueSkolemss` and the sealing in
//! `InlineCall.expand`).

use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::stability::is_stable_path;
use syntax::ast::{Span, TName};
use tasty::{Binder, SymbolId, Type, TypeId, Universe};

/// The skolem standing for each argument, of types `args`, in type `actual` of an application:
/// the one of the argument's widened type, checked by substituting them for parameters `params`
/// of binder `binder` in the method's result type `result`, as `applied_result` does; `None` if
/// that does not give a type equivalent to `actual`.
pub fn call_skolems(
    u: &mut Universe,
    (result, actual): (TypeId, TypeId),
    (binder, params): (Binder, &[SymbolId]),
    args: &[TypeId],
) -> Option<Vec<Option<TypeId>>> {
    let mut skolems = Vec::new();
    u.map_type(actual, &mut |u, part| {
        if matches!(u.types().get(part), Type::Skolem(..)) && !skolems.contains(&part) {
            skolems.push(part);
        }
        None
    });
    let mut chosen = Vec::new();
    for &a in args {
        let w = u.widen(a);
        let found: Vec<TypeId> = skolems
            .iter()
            .copied()
            .filter(|&s| matches!(*u.types().get(s), Type::Skolem(_, i) if i == w))
            .collect();
        chosen.push(match found[..] {
            [s] if !is_stable_path(u, a) => Some(s),
            _ => None,
        });
    }
    let substituted = u.map_type(result, &mut |u, part| {
        let i = match *u.types().get(part) {
            Type::Param(b, i) if b == binder => i as usize,
            Type::Sym(None, s) => params.iter().position(|&p| p == s)?,
            _ => return None,
        };
        chosen.get(i).copied().flatten().or_else(|| args.get(i).copied())
    });
    let same =
        substituted == actual || u.is_subtype(substituted, actual) && u.is_subtype(actual, substituted);
    same.then_some(chosen)
}

impl Inlining<'_> {
    /// `expr.$asInstanceOf$[to]`, at `expr`'s span made synthetic (dotty's `cast`).
    pub fn cast(&mut self, expr: TNodeId, to: TypeId) -> Result<TNodeId, Unsupported> {
        let span = self.tpd.span(expr).to_synthetic();
        let any = self.u.builtins().any;
        let found = self.u.decl(any, "$asInstanceOf$").ok().and_then(|d| d.first().copied());
        let m = found.ok_or((span, "a cast without Any.$asInstanceOf$"))?;
        let prefix = self.tpd.tpe(expr);
        let select_tpe = self.u.intern_type(Type::Sym(Some(prefix), m));
        let name = TName { name: self.names.intern("$asInstanceOf$"), is_type: false };
        let fun = self.tpd.node(T::Select { qualifier: expr, name }, span, Some(select_tpe), Some(m));
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, span, Some(to), None);
        let args = self.tpd.list(&[tpt]);
        Ok(self.tpd.node(T::TypeApply { fun, args }, span, Some(to), None))
    }

    /// The skolem of argument `arg`: none if it applies a method to inferred type arguments and
    /// then to more arguments or defines a class, refused if it has inferred types elsewhere than
    /// in a synthetic ascription or in an inline expansion.
    pub fn arg_skolem(&self, arg: TNodeId, skolem: Option<TypeId>) -> Result<Option<TypeId>, Unsupported> {
        let Some(skolem) = skolem.filter(|_| !self.defines_class(arg)) else { return Ok(None) };
        let is_inferred = |c: &TNodeId| matches!(self.tpd.tree(*c), T::TypeTree { inferred: true });
        let (mut dropped, mut other) = (false, false);
        let mut stack = vec![arg];
        while let Some(t) = stack.pop() {
            let children = self.tpd.children(t);
            if let T::Inlined { .. } = self.tpd.tree(t) {
                continue;
            }
            if let T::TypeApply { args, .. } = *self.tpd.tree(t) {
                let targs = self.tpd.items(args);
                stack.extend(children.into_iter().filter(|c| !targs.contains(c)));
                dropped |= targs.iter().any(is_inferred)
                    && matches!(self.u.types().get(self.tpd.tpe(t)), Type::Method { .. });
            } else {
                let avoided = matches!(self.tpd.tree(t), T::Typed { .. }) && self.tpd.span(t).is_synthetic();
                other |= !avoided && children.iter().any(is_inferred);
                stack.extend(children);
            }
        }
        if dropped {
            Ok(None)
        } else if other {
            Err((self.tpd.span(arg), "skolemized inline argument with an inferred type"))
        } else {
            Ok(Some(skolem))
        }
    }

    /// `expansion` cast to `tpe` if `tpe` refers to a skolem the type of one of `bindings` is.
    pub fn sealed(
        &mut self,
        bindings: &[SymbolId],
        expansion: TNodeId,
        tpe: TypeId,
    ) -> Result<TNodeId, Unsupported> {
        let mut skolems = Vec::new();
        for &b in bindings {
            let info = self.u.info(b);
            let info = match *self.u.types().get(info) {
                Type::ByName(r) => r,
                _ => info,
            };
            if let Type::Skolem(binder, _) = *self.u.types().get(info) {
                skolems.push(binder);
            }
        }
        let mut mentioned = false;
        self.u.map_type(tpe, &mut |u, part| {
            mentioned |= matches!(*u.types().get(part), Type::Skolem(b, _) if skolems.contains(&b));
            None
        });
        if !mentioned {
            return Ok(expansion);
        }
        let span: Span = self.tpd.span(expansion);
        let cast = self.cast(expansion, tpe)?;
        self.tpd.get_mut(cast).span = span;
        Ok(cast)
    }
}
