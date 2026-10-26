// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! `X.copy(original)(...)` of `quotes.reflect` (dotty's `tpd.cpy`): the original if its parts
//! are unchanged, else a tree of its kind typed for the new parts.

use super::tree_kinds::Kind;
use super::{Answer, UNKNOWN, arg, numbers};
use crate::host::Host;
use tasty::{NOTHING, TypeId};
use typer::inlining::Reflect;
use typer::tpd::{TNodeId, Tree as T};

impl Host {
    /// Runs `copy` on the original's expression and its new parts (`-1` for none), the name in
    /// `text`.
    pub fn run_copy(&mut self, op: &str, args: &[i32], text: &str) -> Answer {
        if op != "copy" {
            return Err(UNKNOWN);
        }
        let original = self.tree(arg(args, 0)? as u32)?;
        let kind = self.kind(original);
        let (old, old_name) = self.parts(original, kind)?;
        let new = args[1..].iter().map(|&c| (c >= 0).then(|| self.tree(c as u32)).transpose());
        let new = new.collect::<Result<Vec<_>, _>>()?;
        let named = !matches!(kind, Kind::If | Kind::Match | Kind::SummonFrom | Kind::Unapply);
        let renamed = named && old_name.as_deref().is_some_and(|n| n != text);
        if let T::TypeTree { .. } = self.tpd.tree(original) {
            let bounds = self.u.types().get(self.tpd.tpe(original)).clone();
            let same = match (bounds, &new[..]) {
                (tasty::Type::Bounds(lo, hi), &[Some(l), Some(h)]) => {
                    [lo, hi] == [l, h].map(|b| self.tpd.tpe(b))
                }
                (tasty::Type::Alias(a), &[Some(l), Some(h)]) => [a, a] == [l, h].map(|b| self.tpd.tpe(b)),
                _ => new.is_empty(),
            };
            return match same {
                true => numbers(vec![arg(args, 0)?]),
                false => Err("a copy of a type tree with other parts"),
            };
        }
        if new == old && !renamed {
            return numbers(vec![arg(args, 0)?]);
        }
        if new.len() != old.len() {
            return Err("X.copy with another number of parts than the original's");
        }
        let t = self.copied(original, kind, &new, renamed.then_some(text))?;
        numbers(vec![self.add_tree(t) as i32])
    }

    /// A tree of kind `kind` like `original` with parts `new` and name `renamed` if given.
    fn copied(
        &mut self,
        original: TNodeId,
        kind: Kind,
        new: &[Option<TNodeId>],
        renamed: Option<&str>,
    ) -> Result<TNodeId, &'static str> {
        let span = self.tpd.span(original);
        let part = |i: usize| new[i].ok_or("X.copy without a part the tree needs");
        let rest = |i: usize| new[i..].iter().map(|c| c.ok_or("X.copy with an empty part")).collect();
        let rest: &dyn Fn(usize) -> Result<Vec<TNodeId>, &'static str> = &rest;
        let unit = self.u.class_ref("scala.Unit");
        let tree = self.tpd.tree(original).clone();
        let mut r = Reflect { u: &mut self.u, tpd: &mut self.tpd, names: &mut self.names, span };
        if renamed.is_some() && kind != Kind::Select {
            return Err(refused(kind));
        }
        Ok(match (kind, tree) {
            (Kind::Select, _) => {
                let qual = part(0)?;
                let m = match renamed {
                    Some(name) => {
                        let found = r.u.member_of_type(r.tpd.tpe(qual), name);
                        let terms: Vec<_> =
                            found.into_iter().filter(|&s| r.u.symbol(s).kind.is_term()).collect();
                        let [m] = terms[..] else { return Err("Select.copy to no or an overloaded member") };
                        m
                    }
                    None => r.tpd.sym(original).ok_or("Select.copy of a selection without a symbol")?,
                };
                r.select(qual, m)?
            }
            (Kind::Apply, _) => r.apply(part(0)?, &rest(1)?)?,
            (Kind::TypeApply, _) => r.type_apply(part(0)?, &rest(1)?)?,
            (Kind::Typed | Kind::TypedOrTest, _) => r.typed(part(0)?, part(1)?),
            (Kind::Block, _) => r.block(&rest(1)?, part(0)?)?,
            (Kind::Repeated, _) => r.repeated(&rest(1)?, part(0)?),
            (Kind::NamedArg, T::NamedArg { name, .. }) => {
                node(&mut r, T::NamedArg { name, arg: part(0)? }, None)
            }
            (Kind::Assign, _) => node(&mut r, T::Assign { lhs: part(0)?, rhs: part(1)? }, Some(unit)),
            (Kind::While, _) => node(&mut r, T::WhileDo { cond: part(0)?, body: part(1)? }, Some(unit)),
            (Kind::New, _) => node(&mut r, T::New { tpt: part(0)? }, None),
            (Kind::If, T::If { inline, .. }) => {
                let (thenp, elsep) = (part(1)?, part(2)?);
                let tpe = typer::tpd::lub_of(r.tpd, r.u, &[thenp, elsep]);
                node(&mut r, T::If { cond: part(0)?, thenp, elsep, inline }, Some(tpe))
            }
            (Kind::Match | Kind::SummonFrom, T::Match { kind: m, selector, .. }) => {
                let (selector, from) = if selector.is_some() { (Some(part(0)?), 1) } else { (None, 0) };
                let cases = rest(from)?;
                let tpe = typer::tpd::lub_of(r.tpd, r.u, &cases);
                let cases = r.tpd.list(&cases);
                node(&mut r, T::Match { selector, cases, kind: m }, Some(tpe))
            }
            (Kind::Try, _) => {
                let (expr, cases) = (part(0)?, rest(2)?);
                let tpe = typer::tpd::lub_of(r.tpd, r.u, &[&[expr], &cases[..]].concat());
                let cases = r.tpd.list(&cases);
                node(&mut r, T::Try { expr, cases, finalizer: new[1] }, Some(tpe))
            }
            (Kind::Return, T::Return { from, .. }) if new[1] == Some(from) => {
                node(&mut r, T::Return { expr: part(0)?, from }, Some(NOTHING))
            }
            (Kind::Inlined, T::Inlined { call, .. }) if new[0] == call => {
                let (expansion, bindings) = (part(1)?, rest(2)?);
                let avoided = r.block(&bindings, expansion)?;
                let tpe = r.tpd.tpe(avoided);
                let bindings = r.tpd.list(&bindings);
                node(&mut r, T::Inlined { call, bindings, expansion }, Some(tpe))
            }
            (Kind::CaseDef, _) => {
                let typed = r.tpd.get(original).tpe.is_some();
                let (pat, guard, body) = (part(0)?, new[1], part(2)?);
                let tpe = typed.then(|| r.tpd.tpe(body));
                node(&mut r, T::CaseDef { pat, guard, body }, tpe)
            }
            _ => return Err(refused(kind)),
        })
    }
}

/// A node `tree` at the reflection's span, of type `tpe` or else of its last part's.
fn node(r: &mut Reflect<'_>, tree: T, tpe: Option<TypeId>) -> TNodeId {
    let tpe = tpe.or_else(|| match tree {
        T::NamedArg { arg: last, .. } | T::New { tpt: last } | T::Inlined { expansion: last, .. } => {
            r.tpd.get(last).tpe
        }
        _ => None,
    });
    r.tpd.node(tree, r.span, tpe, None)
}

/// The refusal of a copy of a tree of kind `kind` with other parts.
fn refused(kind: Kind) -> &'static str {
    match kind {
        Kind::Ident | Kind::TypeIdent => "Ident.copy or TypeIdent.copy under another name",
        Kind::Super => "Super.copy with other parts",
        Kind::Closure => "Closure.copy with other parts",
        Kind::Return => "Return.copy from another method",
        Kind::Inlined => "Inlined.copy with another call",
        Kind::Bind | Kind::Unapply | Kind::Alternatives => "a copy of a pattern with other parts",
        Kind::This => "This.copy with another qualifier",
        Kind::Import | Kind::Export | Kind::PackageClause => {
            "Import.copy or PackageClause.copy with other parts"
        }
        Kind::TypeDef | Kind::ValDef | Kind::DefDef | Kind::ClassDef => "a copy of a definition of this kind",
        k if k.is_type() => "a copy of a type tree with other parts",
        _ => "X.copy of a tree of this kind with other parts",
    }
}
