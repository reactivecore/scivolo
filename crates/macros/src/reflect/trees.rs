// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The trees of `quotes.reflect`: their shapes for the extractors (a tree's kind, the expression
//! numbers of its children in the extractor's order, `-1` for none, and its name), symbols,
//! printing, owners and conditionals.

use super::tree_kinds::Kind;
use super::{Answer, UNKNOWN, arg, numbers, symbol, text};
use crate::host::Host;
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Tree as T};

impl Host {
    /// Runs tree operation `op` on `args`.
    pub fn run_tree(&mut self, op: &str, args: &[i32], _: &str) -> Answer {
        let t = || Ok::<_, &'static str>(arg(args, 0)? as u32);
        match op {
            "treeKind" => numbers(vec![self.kind(self.tree(t()?)?) as i32]),
            "tree" => self.shape(self.tree(t()?)?),
            "showTree" => text(self.show_code(self.tree(t()?)?, false)?),
            "showStructure" => text(self.show_code(self.tree(t()?)?, true)?),
            "treeSymbol" => {
                let t = self.tree(t()?)?;
                numbers(vec![self.tree_symbol(t)?.map_or(-1, |s| s.0 as i32)])
            }
            "changeOwner" => {
                let tree = self.tree(t()?)?;
                self.reflect().change_owner(tree, symbol(arg(args, 1)?)?);
                numbers(vec![arg(args, 0)?])
            }
            "if" => {
                let trees = args.iter().map(|&t| self.tree(t as u32)).collect::<Result<Vec<_>, _>>()?;
                let &[cond, thenp, elsep] = &trees[..] else { return Err("If.apply without its parts") };
                let tpe = typer::tpd::lub_of(&self.tpd, &mut self.u, &[thenp, elsep]);
                let tree = T::If { cond, thenp, elsep, inline: false };
                let t = self.tpd.node(tree, self.expansion, Some(tpe), None);
                numbers(vec![self.add_tree(t) as i32])
            }
            _ => Err(UNKNOWN),
        }
    }

    /// `Tree.symbol` (dotty's `Tree.denot`): a definition's or reference's own symbol, else the
    /// symbol its named or `this` type denotes; a proxy tree's forwardee's; none for others.
    fn tree_symbol(&mut self, t: TNodeId) -> Result<Option<SymbolId>, &'static str> {
        match *self.tpd.tree(t) {
            T::Apply { fun, .. } | T::TypeApply { fun, .. } | T::UnApply { fun, .. } => self.tree_symbol(fun),
            T::Typed { expr: inner, .. }
            | T::Super { qual: inner, .. }
            | T::AppliedTypeTree { tpt: inner, .. }
            | T::Annotated { arg: inner, .. } => self.tree_symbol(inner),
            T::Ident { .. }
            | T::Select { .. }
            | T::This { .. }
            | T::Bind { .. }
            | T::ValDef { .. }
            | T::DefDef { .. }
            | T::TypeDef { .. }
            | T::TypeTree { .. }
            | T::SingletonTypeTree { .. } => Ok(self.tpd.sym(t).or_else(|| self.denoted(t))),
            T::Literal { .. }
            | T::ClassLiteral { .. }
            | T::New { .. }
            | T::NamedArg { .. }
            | T::Closure { .. }
            | T::SeqLiteral { .. }
            | T::Assign { .. }
            | T::Block { .. }
            | T::If { .. }
            | T::CaseDef { .. }
            | T::Return { .. }
            | T::WhileDo { .. }
            | T::Try { .. }
            | T::Match { .. }
            | T::Alternative { .. }
            | T::ByNameTypeTree { .. }
            | T::TypeBoundsTree { .. }
            | T::LambdaTypeTree { .. }
            | T::Inlined { .. } => Ok(None),
            _ => Err("Tree.symbol of a tree the frontend does not map"),
        }
    }

    /// The symbol the named or `this` type of `t` denotes.
    fn denoted(&mut self, t: TNodeId) -> Option<SymbolId> {
        let tpe = self.tpd.get(t).tpe?;
        match *self.u.types().get(tpe) {
            Type::Sym(_, s) | Type::Package(s) => Some(s),
            Type::TypeRef(..) | Type::TermRef(..) => self.u.resolve_ref(tpe),
            Type::This(c) => self.u.class_symbol(c),
            _ => None,
        }
    }

    /// The kind of `t`, its children and its name.
    fn shape(&mut self, t: TNodeId) -> Answer {
        let kind = self.kind(t);
        let (mut children, name) = self.parts(t, kind)?;
        if let (Kind::TypeBoundsTree, T::TypeTree { .. }) = (kind, self.tpd.tree(t)) {
            children = self.bounds_trees(t).into_iter().map(Some).collect();
        }
        let children = children.into_iter().map(|c| c.map_or(-1, |c| self.add_tree(c) as i32));
        Ok(([kind as i32].into_iter().chain(children).collect(), name))
    }

    /// Type trees of the bounds of type tree `t` of a bounds or alias type, at its span.
    fn bounds_trees(&mut self, t: TNodeId) -> [TNodeId; 2] {
        let span = self.tpd.span(t);
        let (lo, hi) = match *self.u.types().get(self.tpd.tpe(t)) {
            Type::Bounds(lo, hi) => (lo, hi),
            Type::Alias(a) => (a, a),
            _ => (tasty::NOTHING, tasty::ANY),
        };
        [lo, hi].map(|b| self.tpd.node(T::TypeTree { inferred: false }, span, Some(b), None))
    }
}
