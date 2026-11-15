// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Arguments aligned with the parameters typed, those left out given their defaults (dotty's
//! `matchArgs`), where neither the function nor the arguments need lifting.

use super::reordered_args::Aligned;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::{NameId, SymbolId, TypeId};

impl Typer<'_> {
    /// Call `f` of `method` and its arguments `aligned`, typed against `formals`, the types of
    /// `params`, a missing one its default, with the definitions lifted out of them (dotty's
    /// `TypedApply` with `liftFun` and `liftArgs`).
    pub fn typed_aligned_args(
        &mut self,
        (mut f, method): (TNodeId, Option<SymbolId>),
        (args, aligned, pretyped): (&[NodeId], &Aligned, Option<&[Option<TNodeId>]>),
        (params, formals): (&[(NameId, TypeId)], &[TypeId]),
        ctx: Ctx,
    ) -> Result<(TNodeId, Vec<TNodeId>, Vec<TNodeId>), &'static str> {
        let pre = |a: NodeId| pretyped.and_then(|p| args.iter().position(|&b| b == a).and_then(|i| p[i]));
        let args_typed = pretyped.map_or(args.is_empty(), |p| p.iter().all(Option::is_some));
        let method = method.or_else(|| self.tpd.sym(self.tpd.meth_part(f)));
        let complex = method.is_some_and(|m| self.has_defaults(m));
        let contextual = self.takes_using(f);
        let at = self.tpd.span(f).start_pos();
        let mut defs = Vec::new();
        let lifted_fun = aligned.contains(&None);
        let mut lifted_formals = None;
        if lifted_fun {
            let before = self.tpd.tpe(f);
            f = self.lift_app(&mut defs, f, true, ctx)?;
            if self.tpd.tpe(f) != before {
                lifted_formals = self.lifted_formals(f, formals.len());
            }
        }
        let formals = lifted_formals.as_deref().unwrap_or(formals);
        let mut typed = Vec::new();
        let mut safe = true;
        for (i, slot) in aligned.iter().enumerate() {
            let formal = formals[i];
            let arg = match *slot {
                Some(a) => match *self.ast.tree(a) {
                    Tree::NamedArg { name, arg } => {
                        let arg = self.arg_against(arg, pre(a), formal, ctx);
                        let tpe = self.tpd.get(arg).tpe;
                        self.tpd.node(T::NamedArg { name, arg }, self.ast.span(a), tpe, None)
                    }
                    _ => self.arg_against(a, pre(a), formal, ctx),
                },
                None => {
                    let method = method.ok_or("default argument")?;
                    if contextual && !self.may_default(f, method, i) {
                        typed.push(self.implicit_arg(formal, at, ctx, 0)?);
                        continue;
                    }
                    let getter = self.default_arg(f, (method, i), args_typed, ctx)?;
                    typed.push(self.adapt(getter, Pt::Type(formal), ctx));
                    continue;
                }
            };
            safe &= self.is_pure_expr(arg);
            typed.push(arg);
        }
        if method.is_some_and(|m| self.is_java_annotation_constructor(m)) {
            return Ok((f, self.named_java_annotation_args(params, typed), defs));
        }
        let given = aligned.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
        let in_order = given == args.len() && aligned[..given].iter().zip(args).all(|(a, b)| *a == Some(*b));
        if !in_order && !safe {
            if !lifted_fun {
                f = self.lift_app(&mut defs, f, complex, ctx)?;
            }
            let mut arg_defs = Vec::new();
            typed = self.lift_args(&mut arg_defs, params, &typed, complex, ctx);
            let position =
                |i: usize| aligned[i].and_then(|a| args.iter().position(|&b| b == a)).unwrap_or(args.len());
            arg_defs.sort_by_key(|&(i, _)| position(i));
            defs.extend(arg_defs.into_iter().map(|(_, d)| d));
        }
        Ok((f, typed, defs))
    }

    /// Whether parameter `n` of the clause call `f` of `method` applies has a default argument.
    pub fn may_default(&mut self, mut f: TNodeId, method: SymbolId, mut n: usize) -> bool {
        loop {
            f = match *self.tpd.tree(f) {
                T::Apply { fun, args } => {
                    n += self.tpd.items(args).len();
                    fun
                }
                T::TypeApply { fun, .. } => fun,
                _ => break,
            };
        }
        self.has_default_at(method, n)
    }
}
