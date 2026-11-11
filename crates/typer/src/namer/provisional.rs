// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The provisional info of a type definition while its right-hand side is typed, so that it may
//! refer to itself (dotty's `typeDefSig` dummy info).

use crate::ctx::Ctx;
use crate::symbols::Completion;
use crate::typer::Typer;
use syntax::ast::{NodeId, Tree};
use tasty::{ANY, NOTHING, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Gives type definition `sym` of `tree`, not a class, the info of an abstract type over its
    /// type parameters, a match type alias one bounded by its declared bound; nothing for
    /// another definition.
    pub fn set_provisional_info(&mut self, sym: SymbolId, tree: NodeId, ctx: Ctx) {
        let Some(tparams) = self.type_def_params(tree) else { return };
        let info = self.abstract_info(&tparams, ANY);
        self.u.set_info(sym, info);
        if !self.is_match_alias(tree) {
            return;
        }
        let upper = self.provisional_bound(tree, &tparams, ctx.with_owner(sym));
        if upper != ANY {
            let info = self.abstract_info(&tparams, upper);
            self.u.set_info(sym, info);
        }
    }

    /// The info of an abstract type over `tparams` bounded by `upper`.
    fn abstract_info(&mut self, tparams: &[NodeId], upper: TypeId) -> TypeId {
        let bounds = self.u.intern_type(Type::Bounds(NOTHING, ANY));
        let upper = self.u.intern_type(Type::Bounds(NOTHING, upper));
        match tparams.is_empty() {
            true => upper,
            false => {
                let names: Vec<String> = tparams
                    .iter()
                    .map(|&p| match *self.ast.tree(p) {
                        Tree::TypeDef { name, .. } => self.text(name.name).to_owned(),
                        _ => String::new(),
                    })
                    .collect();
                let params: Vec<_> = names.iter().map(|n| (self.u.intern_name(n), bounds)).collect();
                let binder = self.syms.fresh_binder();
                self.u.intern_lambda(binder, &params, upper)
            }
        }
    }

    /// The bound of match type alias `tree` if it names a type other than one of `tparams`, typed
    /// ahead without the diagnostics inside it; else `Any`.
    fn provisional_bound(&mut self, tree: NodeId, tparams: &[NodeId], ctx: Ctx) -> TypeId {
        let Tree::TypeDef { rhs, .. } = *self.ast.tree(tree) else { return ANY };
        let body = match *self.ast.tree(rhs) {
            Tree::LambdaTypeTree { body, .. } => body,
            _ => rhs,
        };
        let Tree::MatchTypeTree { bound: Some(b), .. } = *self.ast.tree(body) else { return ANY };
        let named = match *self.ast.tree(b) {
            Tree::Ident { name } => !tparams.iter().any(|&p| {
                matches!(*self.ast.tree(p), Tree::TypeDef { name: n, .. } if self.text(n.name) == self.text(name.name))
            }),
            Tree::Select { .. } => true,
            _ => false,
        };
        if !named {
            return ANY;
        }
        let diags = self.diags.items.len();
        let typed = self.typed_type(b, ctx);
        let tpe = self.tpd.tpe(typed);
        let failed = self.diags.items.len() > diags || matches!(self.u.ty(tpe), Type::Error);
        let span = self.ast.span(b);
        let mut i = 0;
        self.diags.items.retain(|d| {
            i += 1;
            i <= diags || d.offset.is_none_or(|o| o < span.start() || o > span.end())
        });
        if failed { ANY } else { tpe }
    }

    /// Whether definition `tree` is a match type alias.
    pub fn is_match_alias(&self, tree: NodeId) -> bool {
        self.match_alias_params(tree).is_some()
    }

    /// Whether `t` refers to a type definition being completed, or to a local one not completed
    /// yet (dotty's `isProvisional`).
    pub fn is_provisional_type(&mut self, t: TypeId) -> bool {
        let mut refs = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Some(s) = u.sym_of(part)
                && u.is_type_symbol(s)
                && !u.is_class(s)
            {
                refs.push(s);
            }
            None
        });
        refs.into_iter().any(|s| match self.syms.completions.get(&s) {
            Some(Completion::Completing(_)) => true,
            Some(Completion::Pending { .. }) => !self.u.is_static_symbol(s),
            _ => false,
        })
    }

    /// The type parameters of definition `tree` if it is a match type alias.
    fn match_alias_params(&self, tree: NodeId) -> Option<Vec<NodeId>> {
        let Tree::TypeDef { rhs, .. } = *self.ast.tree(tree) else { return None };
        let body = match *self.ast.tree(rhs) {
            Tree::LambdaTypeTree { body, .. } => body,
            _ => rhs,
        };
        matches!(self.ast.tree(body), Tree::MatchTypeTree { .. }).then(|| self.type_def_params(tree))?
    }

    /// The type parameters of definition `tree` if it is a type definition other than a class.
    fn type_def_params(&self, tree: NodeId) -> Option<Vec<NodeId>> {
        let Tree::TypeDef { rhs, .. } = *self.ast.tree(tree) else { return None };
        match *self.ast.tree(rhs) {
            Tree::Template { .. } => None,
            Tree::LambdaTypeTree { tparams, .. } => Some(self.ast.items(tparams).to_vec()),
            _ => Some(Vec::new()),
        }
    }
}
