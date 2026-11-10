// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeApplications.scala
//! The arities of type constructors (dotty's `typeParams`, `hkTypeParams`) and the bounds of
//! type parameters of any kind.

use super::Typer;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// How many type parameters type constructor `t` takes: a class's, a type lambda's, a type
    /// member's as the refinements of its prefix give it.
    pub fn type_arity(&mut self, t: TypeId) -> usize {
        self.type_arity_within(t, 32)
    }

    fn type_arity_within(&mut self, t: TypeId, depth: u32) -> usize {
        let Some(depth) = depth.checked_sub(1) else { return 0 };
        match self.u.types().get(t).clone() {
            Type::Sym(pre, s) => match self.u.symbol(s).kind {
                Kind::Class if self.is_pending_plain_class(s) => 0,
                Kind::Class => {
                    if matches!(
                        self.syms.completions.get(&s),
                        Some(crate::symbols::Completion::Pending { .. })
                    ) {
                        self.complete(s);
                    }
                    self.u.type_params(s).len()
                }
                Kind::Type | Kind::TypeParam => {
                    if let Some(n) = self.arity_while_completing(s) {
                        return n;
                    }
                    self.complete(s);
                    if let Some(n) = self.completing_arity(s) {
                        return n;
                    }
                    let info = self.types().refined_info(pre, s);
                    self.info_arity(info, depth)
                }
                _ => 0,
            },
            Type::Lambda { params, .. } => params.len(),
            Type::TypeRef(pre, name) => match self.u.refinement_named(pre, &name) {
                Some(info) => self.info_arity(info, depth),
                None => self.u.resolve_ref(t).map_or(0, |s| {
                    let sym = self.u.intern_type(Type::Sym(Some(pre), s));
                    self.type_arity_within(sym, depth)
                }),
            },
            Type::Applied(tycon, _) => {
                if let Type::Sym(_, s) = *self.u.types().get(tycon) {
                    self.complete(s);
                }
                let reduced = self.u.dealias(t);
                if reduced == t { 0 } else { self.type_arity_within(reduced, depth) }
            }
            _ => 0,
        }
    }

    /// Whether `t` refers to a type symbol being completed.
    pub fn is_completing_type(&mut self, t: TypeId) -> bool {
        matches!(*self.u.types().get(t), Type::Sym(_, s) if self.u.is_type_symbol(s)
            && matches!(self.syms.completions.get(&s), Some(crate::symbols::Completion::Completing(_))))
    }

    /// Whether `s` is a class without type parameters not completed yet.
    pub fn is_pending_plain_class(&self, s: SymbolId) -> bool {
        match self.syms.completions.get(&s) {
            Some(&crate::symbols::Completion::Pending { tree, .. }) => self.pending_class_arity(tree) == 0,
            _ => false,
        }
    }

    /// How many type parameters the class that `tree` defines declares (dotty's
    /// `completerTypeParams`).
    fn pending_class_arity(&self, tree: NodeId) -> usize {
        let Tree::TypeDef { rhs, .. } = *self.ast.tree(tree) else { return 0 };
        let Tree::Template { body, .. } = *self.ast.tree(rhs) else { return 0 };
        let is_param = |&&s: &&NodeId| match *self.ast.tree(s) {
            Tree::TypeDef { mods, .. } => self.ast.modifiers(mods).flags.is(syntax::ast::flags::TYPE_PARAM),
            _ => false,
        };
        self.ast.items(body).iter().take_while(is_param).count()
    }

    /// How many type parameters type member `s` declares while its info is completed and is not
    /// provisional (dotty's `completerTypeParams`).
    fn arity_while_completing(&self, s: SymbolId) -> Option<usize> {
        let Some(&crate::symbols::Completion::Completing(tree)) = self.syms.completions.get(&s) else {
            return None;
        };
        let Tree::TypeDef { rhs, .. } = *self.ast.tree(tree) else { return None };
        match *self.ast.tree(rhs) {
            _ if self.is_match_alias(tree) => None,
            Tree::LambdaTypeTree { tparams, .. } => Some(self.ast.items(tparams).len()),
            _ => Some(0),
        }
    }

    /// How many type parameters the definition of `s` being completed, or of type parameter `s`,
    /// declares, `F[X] <: …`.
    fn completing_arity(&self, s: SymbolId) -> Option<usize> {
        let tree = match self.syms.completions.get(&s) {
            Some(&crate::symbols::Completion::Completing(tree)) => tree,
            _ => *self.syms.param_trees.get(&s)?,
        };
        let Tree::TypeDef { rhs, .. } = *self.ast.tree(tree) else { return None };
        let Tree::LambdaTypeTree { tparams, .. } = *self.ast.tree(rhs) else { return None };
        Some(self.ast.items(tparams).len())
    }

    /// How many type parameters a type member of info `info` takes.
    fn info_arity(&mut self, info: TypeId, depth: u32) -> usize {
        match *self.u.types().get(info) {
            Type::Alias(a) | Type::Bounds(_, a) => self.type_arity_within(a, depth),
            Type::Lambda { ref params, .. } => params.len(),
            _ => 0,
        }
    }

    /// The lower and upper bound of type parameter info `bounds` and the arity of the parameter;
    /// of a higher-kinded one, `[X] =>> >: L <: H`, `[X] =>> L` and `[X] =>> H`, the lower one
    /// `None` for `Nothing`.
    pub fn bounds_of(&mut self, bounds: TypeId) -> (Option<TypeId>, TypeId, usize) {
        match self.u.types().get(bounds).clone() {
            Type::Bounds(lo, hi) => (Some(lo), hi, self.type_arity(hi)),
            Type::Lambda { binder, params, result } => {
                let (lo, hi) = match *self.u.types().get(result) {
                    Type::Bounds(lo, hi) => (lo, hi),
                    _ => (result, result),
                };
                let bottom = self.u.class_symbol(lo) == Some(self.u.builtins().nothing);
                let arity = params.len();
                let mut lambda = |result| {
                    let params = params.clone();
                    self.u.intern_type(Type::Lambda { binder, params, result })
                };
                let lo = lambda(lo);
                ((!bottom).then_some(lo), lambda(hi), arity)
            }
            _ => (Some(bounds), bounds, self.type_arity(bounds)),
        }
    }
}
