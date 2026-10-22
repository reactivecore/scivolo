// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The shapes of trees for the extractors of `quotes.reflect`: a tree's kind, the expression
//! numbers of its children in the extractor's order (`-1` for none) and its name.

use super::{Answer, UNKNOWN, arg, numbers};
use crate::host::Host;
use typer::tpd::{TNodeId, Tree as T};

/// The kinds of trees, as the overlay's `ReflectOps.TreeKind` numbers them.
#[derive(Clone, Copy)]
enum Kind {
    Other = 0,
    Ident = 1,
    Select = 2,
    Literal = 3,
    Typed = 4,
    Apply = 5,
    TypeApply = 6,
    Block = 7,
    Inlined = 8,
    Repeated = 9,
    TypeTree = 10,
    Inferred = 11,
    ValDef = 12,
    NamedArg = 13,
    Term = 14,
    DefDef = 15,
    TypeDef = 16,
    Import = 17,
}

impl Host {
    /// Runs tree operation `op` on `args`.
    pub fn run_tree(&mut self, op: &str, args: &[i32], _: &str) -> Answer {
        let t = || Ok::<_, &'static str>(arg(args, 0)? as u32);
        match op {
            "treeKind" => numbers(vec![self.kind(self.tree(t()?)?) as i32]),
            "tree" => self.shape(self.tree(t()?)?),
            "treeSymbol" => numbers(vec![self.tpd.sym(self.tree(t()?)?).map_or(-1, |s| s.0 as i32)]),
            _ => Err(UNKNOWN),
        }
    }

    fn kind(&self, t: TNodeId) -> Kind {
        let is_type = self.tpd.sym(t).is_some_and(|s| self.u.symbol(s).kind.is_type());
        match *self.tpd.tree(t) {
            T::Ident { .. } if is_type => Kind::TypeTree,
            T::Select { .. } if is_type => Kind::TypeTree,
            T::Ident { .. } => Kind::Ident,
            T::Select { .. } => Kind::Select,
            T::Literal { .. } | T::ClassLiteral { .. } => Kind::Literal,
            T::Typed { .. } => Kind::Typed,
            T::Apply { .. } => Kind::Apply,
            T::TypeApply { .. } => Kind::TypeApply,
            T::Block { .. } => Kind::Block,
            T::Inlined { .. } => Kind::Inlined,
            T::SeqLiteral { .. } => Kind::Repeated,
            T::TypeTree { inferred: true } => Kind::Inferred,
            T::TypeTree { .. }
            | T::SingletonTypeTree { .. }
            | T::ByNameTypeTree { .. }
            | T::AppliedTypeTree { .. }
            | T::LambdaTypeTree { .. } => Kind::TypeTree,
            T::Annotated { .. } => Kind::TypeTree,
            T::ValDef { .. } => Kind::ValDef,
            T::NamedArg { .. } => Kind::NamedArg,
            T::This { .. }
            | T::Super { .. }
            | T::New { .. }
            | T::Closure { .. }
            | T::Assign { .. }
            | T::If { .. }
            | T::Return { .. }
            | T::WhileDo { .. }
            | T::Try { .. }
            | T::Match { .. }
            | T::Labeled { .. } => Kind::Term,
            T::DefDef { .. } => Kind::DefDef,
            T::TypeDef { .. } => Kind::TypeDef,
            T::Import { .. } => Kind::Import,
            _ => Kind::Other,
        }
    }

    /// The kind of `t`, its children and its name.
    fn shape(&mut self, t: TNodeId) -> Answer {
        let kind = self.kind(t);
        let tree = self.tpd.tree(t).clone();
        let items = |l| self.tpd.items(l).to_vec();
        let (children, name): (Vec<Option<TNodeId>>, _) = match (kind, tree) {
            (Kind::Ident, T::Ident { name }) => (Vec::new(), Some(name)),
            (Kind::Select, T::Select { qualifier, name }) => (vec![Some(qualifier)], Some(name)),
            (_, T::Typed { expr, tpt }) => (vec![Some(expr), Some(tpt)], None),
            (_, T::Apply { fun, args } | T::TypeApply { fun, args }) => {
                ([Some(fun)].into_iter().chain(items(args).into_iter().map(Some)).collect(), None)
            }
            (_, T::Block { stats, expr }) => {
                ([Some(expr)].into_iter().chain(items(stats).into_iter().map(Some)).collect(), None)
            }
            (_, T::Inlined { call, bindings, expansion }) => {
                let bindings = items(bindings).into_iter().map(Some);
                ([call, Some(expansion)].into_iter().chain(bindings).collect(), None)
            }
            (_, T::SeqLiteral { elems, elemtpt }) => {
                ([Some(elemtpt)].into_iter().chain(items(elems).into_iter().map(Some)).collect(), None)
            }
            (_, T::ValDef { name, tpt, rhs }) => (vec![Some(tpt), rhs], Some(name)),
            (_, T::NamedArg { name, arg }) => (vec![Some(arg)], Some(name)),
            (_, T::DefDef { name, .. } | T::TypeDef { name, .. }) => (Vec::new(), Some(name)),
            _ => (Vec::new(), None),
        };
        let name = name.map(|n| self.names.text(n.name).to_owned());
        let children = children.into_iter().map(|c| c.map_or(-1, |c| self.add_tree(c) as i32));
        Ok(([kind as i32].into_iter().chain(children).collect(), name))
    }
}
