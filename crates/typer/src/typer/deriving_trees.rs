//! The untyped trees of derived type class instances: type parameters copied from the deriving
//! class, applied type class and class types.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// A new untyped node of `tree` at `span`.
    pub fn derived_node(&mut self, tree: Tree, span: Span) -> NodeId {
        let node = self.ast.node(tree);
        self.ast.with_span(node, span);
        node
    }

    /// An untyped node standing for a type tree of `t` at `span`.
    pub fn derived_type(&mut self, t: TypeId, span: Span) -> NodeId {
        let node = self.splice_type(t);
        self.ast.with_span(node, span);
        node
    }

    /// `tc[args]`.
    pub fn tc_applied(&mut self, tc: TypeId, args: Vec<NodeId>, span: Span) -> NodeId {
        let tpt = self.derived_type(tc, span);
        let args = self.ast.list(&args);
        self.derived_node(Tree::AppliedTypeTree { tpt, args }, span)
    }

    /// `class[args]`, or `class` without arguments.
    pub fn class_applied(&mut self, class: TypeId, args: Vec<NodeId>, span: Span) -> NodeId {
        if args.is_empty() {
            return self.derived_type(class, span);
        }
        self.tc_applied(class, args, span)
    }

    /// Typed instance definition `def` with plain type trees for its parameters' and result's
    /// types, as dotty's `tpd.DefDef` of a symbol has.
    pub fn with_plain_type_trees(&mut self, def: TNodeId) -> TNodeId {
        let plain = |t: &mut Self, n: TNodeId| t.tpd.get_mut(n).tree = T::TypeTree { inferred: false };
        match *self.tpd.tree(def) {
            T::ValDef { tpt, .. } => plain(self, tpt),
            T::DefDef { paramss, tpt, .. } => {
                let params: Vec<TNodeId> =
                    self.tpd.clause_items(paramss).iter().flat_map(|&c| self.tpd.items(c).to_vec()).collect();
                for p in params {
                    match *self.tpd.tree(p) {
                        T::ValDef { tpt, .. } => plain(self, tpt),
                        T::TypeDef { rhs, .. } => plain(self, rhs),
                        _ => {}
                    }
                }
                plain(self, tpt);
            }
            _ => {}
        }
        def
    }

    /// A reference to type parameter `p` by its name.
    pub fn param_ident(&mut self, p: SymbolId, span: Span) -> NodeId {
        let name = self.u.symbol(p).name.clone();
        let name = TName { name: self.name(&name), is_type: true };
        self.derived_node(Tree::Ident { name }, span)
    }

    /// The definition of a type parameter like class type parameter `p`.
    pub fn param_def_of(&mut self, p: SymbolId, span: Span) -> Result<NodeId, &'static str> {
        let name = self.u.symbol(p).name.clone();
        let info = self.u.info(p);
        self.param_def(&name, info, span)
    }

    /// The definition of type parameter `name` with info `info`: bounds, or a type lambda over
    /// bounds for a higher-kinded one.
    pub fn param_def(&mut self, name: &str, info: TypeId, span: Span) -> Result<NodeId, &'static str> {
        let mut dependent = false;
        self.u.map_type(info, &mut |u, t| {
            dependent |= match *u.types().get(t) {
                Type::Param(..) | Type::Local(..) => true,
                Type::Sym(_, s) => u.symbol(s).kind == Kind::TypeParam,
                _ => false,
            };
            None
        });
        if dependent {
            return Err("derives clause of a class whose type parameter bounds are dependent");
        }
        let lambda = match self.u.types().get(info).clone() {
            Type::Bounds(_, hi) => match self.u.types().get(hi).clone() {
                Type::Lambda { params, result, .. } => Some((params, result)),
                _ => None,
            },
            Type::Lambda { params, result, .. } => Some((params, result)),
            _ => None,
        };
        let rhs = match lambda {
            Some((params, result)) => {
                let mut tparams = Vec::new();
                for (i, (n, b)) in params.iter().enumerate() {
                    let n = if n == "_" { format!("_${}", i + 1) } else { n.clone() };
                    tparams.push(self.param_def(&n, *b, span)?);
                }
                let result = match self.u.types().get(result) {
                    Type::Bounds(..) => result,
                    _ => self.u.intern_type(Type::Bounds(tasty::NOTHING, result)),
                };
                let body = self.derived_type(result, span);
                let tparams = self.ast.list(&tparams);
                self.derived_node(Tree::LambdaTypeTree { tparams, body }, span)
            }
            None => self.derived_type(info, span),
        };
        let name = TName { name: self.name(name), is_type: true };
        let mods = self.ast.mods(Modifiers::with_flags(f::TYPE_PARAM));
        Ok(self.derived_node(Tree::TypeDef { name, rhs, mods }, span))
    }
}
