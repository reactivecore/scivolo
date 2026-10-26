// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The children of a tree in its extractor's order and its name, for the extractors and
//! copies of `quotes.reflect`.

use super::tree_kinds::Kind;
use crate::host::Host;
use syntax::ast::TName;
use typer::tpd::{List, MatchKind, TNodeId, Tree as T};

/// A tree's children (`None` for an empty one) and its name.
pub type Parts = (Vec<Option<TNodeId>>, Option<String>);

impl Host {
    /// The children of `t` of kind `kind` and its name; an `If` or `Match` is named `inline` if
    /// it is one, an `Unapply` by its number of implicit arguments.
    pub fn parts(&self, t: TNodeId, kind: Kind) -> Result<Parts, &'static str> {
        let items = |l| self.tpd.items(l).iter().map(|&c| Some(c));
        let name = |n: TName| Some(self.names.text(n.name).to_owned());
        let with = |first: Vec<Option<TNodeId>>, rest: List| first.into_iter().chain(items(rest)).collect();
        let inline = |yes: bool| yes.then(|| "inline".to_owned());
        Ok(match (kind, self.tpd.tree(t).clone()) {
            (_, T::Ident { name: n }) => (Vec::new(), name(n)),
            (_, T::Select { qualifier, name: n }) => (vec![Some(qualifier)], name(n)),
            (_, T::Typed { expr, tpt }) => (vec![Some(expr), Some(tpt)], None),
            (_, T::Apply { fun, args } | T::TypeApply { fun, args }) => (with(vec![Some(fun)], args), None),
            (_, T::Block { stats, expr }) => (with(vec![Some(expr)], stats), None),
            (_, T::Inlined { call, bindings, expansion }) => {
                (with(vec![call, Some(expansion)], bindings), None)
            }
            (_, T::SeqLiteral { elems, elemtpt }) => (with(vec![Some(elemtpt)], elems), None),
            (_, T::ValDef { name: n, tpt, rhs }) => (vec![Some(tpt), rhs], name(n)),
            (_, T::NamedArg { name: n, arg }) => (vec![Some(arg)], name(n)),
            (_, T::Closure { meth, tpt, .. }) => (vec![Some(meth), tpt], None),
            (_, T::DefDef { name: n, tpt, rhs, .. }) => (vec![Some(tpt), rhs], name(n)),
            (Kind::ClassDef, T::TypeDef { name: n, .. }) => (Vec::new(), name(n)),
            (_, T::TypeDef { name: n, rhs }) => (vec![Some(rhs)], name(n)),
            (_, T::This { qual }) => match *self.tpd.tree(qual) {
                T::Ident { name: n } if !self.names.text(n.name).is_empty() => (Vec::new(), name(n)),
                T::Ident { .. } => (Vec::new(), None),
                _ => return Err("This with a qualifier other than a name"),
            },
            (_, T::Super { qual, mix }) => match *self.tpd.tree(mix) {
                T::Ident { name: n } if !self.names.text(n.name).is_empty() => (vec![Some(qual)], name(n)),
                _ => (vec![Some(qual)], None),
            },
            (_, T::New { tpt }) => (vec![Some(tpt)], None),
            (_, T::Assign { lhs, rhs }) => (vec![Some(lhs), Some(rhs)], None),
            (_, T::If { cond, thenp, elsep, inline: i }) => {
                (vec![Some(cond), Some(thenp), Some(elsep)], inline(i))
            }
            (_, T::Match { selector, cases, kind }) => {
                let first = if selector.is_some() { vec![selector] } else { Vec::new() };
                (with(first, cases), inline(kind == MatchKind::Inline))
            }
            (_, T::Return { expr, from }) => (vec![Some(expr), Some(from)], None),
            (_, T::WhileDo { cond, body }) => (vec![Some(cond), Some(body)], None),
            (_, T::Try { expr, cases, finalizer }) => (with(vec![Some(expr), finalizer], cases), None),
            (_, T::CaseDef { pat, guard, body }) => (vec![Some(pat), guard, Some(body)], None),
            (_, T::Bind { name: n, body }) => (vec![Some(body)], name(n)),
            (_, T::UnApply { fun, implicits, patterns }) => {
                let count = self.tpd.items(implicits).len().to_string();
                (with(with(vec![Some(fun)], implicits), patterns), Some(count))
            }
            (_, T::Alternative { trees }) => (with(Vec::new(), trees), None),
            (_, T::SingletonTypeTree { reference }) => (vec![Some(reference)], None),
            (_, T::ByNameTypeTree { result }) => (vec![Some(result)], None),
            (_, T::Annotated { arg, annot }) => (vec![Some(arg), Some(annot)], None),
            (_, T::AppliedTypeTree { tpt, args }) => (with(vec![Some(tpt)], args), None),
            (_, T::LambdaTypeTree { tparams, body }) => (with(vec![Some(body)], tparams), None),
            (_, T::RefinedTypeTree { tpt, refinements }) => (with(vec![Some(tpt)], refinements), None),
            (_, T::TypeBoundsTree { lo, hi, .. }) => (vec![Some(lo), Some(hi)], None),
            (_, T::Import { expr, .. }) => (vec![Some(expr)], None),
            (_, T::PackageDef { pid, stats }) => (with(vec![Some(pid)], stats), None),
            _ => (Vec::new(), None),
        })
    }
}
