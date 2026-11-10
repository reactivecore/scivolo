// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The members of refined type trees, definitions in a synthetic `<refinement>` class (dotty's
//! `refinedTypeToClass`, `typedRefinedTypeTree`).

use super::Typer;
use crate::ctx::{Ctx, Level};
use crate::tpd::TNodeId;
use syntax::ast::flags::{self as f, GIVEN, IMPLICIT, MUTABLE};
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, NameId, SymbolId, Type, TypeId};

/// A refinement: its name, its info and its typed definition.
pub type Refinement = (String, TypeId, TNodeId);

impl Typer<'_> {
    /// The class the members of a refined type tree typed in `ctx` belong to.
    pub fn refinement_class(&mut self, ctx: Ctx) -> SymbolId {
        let class = self.syms.define(self.u, "<refinement>", Kind::Class, ctx.owner, f::SYNTHETIC);
        self.u.unenter(class);
        class
    }

    /// Type refinement `r` of right-hand side `rhs`, its symbol entered by `refinement_scope`, its
    /// info from `type_def_info`; `Err` with the tree of a refusal.
    pub fn type_refinement(&mut self, r: NodeId, ctx: Ctx) -> Result<Refinement, TNodeId> {
        let Tree::TypeDef { name, rhs, .. } = *self.ast.tree(r) else { unreachable!("a type refinement") };
        let text = self.text(name.name).to_owned();
        let sym = self.syms.by_tree[&r];
        let mctx = Ctx { owner: sym, ..ctx };
        let info = self.type_def_info(rhs, mctx);
        let info = match *self.u.ty(info) {
            Type::Error => return Err(self.unsupported(r, "refinement of an unsupported type")),
            Type::Lambda { .. } => self.u.intern_type(Type::Alias(info)),
            _ => info,
        };
        if self.aliases_itself(sym, info) {
            return Err(self.unsupported(r, "refinement alias referring to itself"));
        }
        if let Some((lo, hi)) = self.u.bounds(info)
            && !self.u.is_subtype(lo, hi)
        {
            return Err(self.unsupported(r, "refinement with conflicting bounds"));
        }
        self.u.set_info(sym, info);
        self.syms.by_tree.insert(r, sym);
        let tree = self.typed_type_def(r, mctx);
        Ok((text, info, tree))
    }

    /// Term refinement `r`, a member of `class`: a value's type, a parameterless method's
    /// by-name result, a method type as its info; `Err` for one that is not a plain value or
    /// method.
    pub fn term_refinement(
        &mut self,
        class: SymbolId,
        r: NodeId,
        ctx: Ctx,
    ) -> Result<Refinement, &'static str> {
        let (name, clauses, tpt, mods) = match *self.ast.tree(r) {
            Tree::ValDef { name, tpt, mods, .. } => (name, None, tpt, mods),
            Tree::DefDef { name, paramss, tpt, mods, .. } => (name, Some(paramss), tpt, mods),
            _ => return Err("structural refinement"),
        };
        let flags = self.ast.modifiers(mods).flags;
        if flags.is_one_of(MUTABLE.or(GIVEN).or(IMPLICIT)) {
            return Err("structural refinement with modifiers");
        }
        let text = self.text(name.name).to_owned();
        let (kind, flags) = match clauses {
            Some(_) => (Kind::Def, flags.or(f::METHOD)),
            None => (Kind::Val, flags),
        };
        let sym = self.syms.define(self.u, &text, kind, class, flags.to_term_flags());
        let mut mctx = Ctx { owner: sym, ..ctx };
        let mut clause_trees = Vec::new();
        let mut clause_params = Vec::new();
        let mut value_clauses = clauses.map(|c| self.ast.clause_items(c).to_vec()).unwrap_or_default();
        let mut tparams = Vec::new();
        if let Some(&first) = value_clauses.first()
            && self
                .ast
                .items(first)
                .first()
                .is_some_and(|&p| matches!(self.ast.tree(p), Tree::TypeDef { .. }))
        {
            let (syms, lctx) =
                self.poly_type_params(first, sym, mctx).map_err(|_| "polymorphic structural refinement")?;
            mctx = Ctx { owner: sym, ..lctx };
            let items = self.ast.items(first).to_vec();
            clause_trees.push(items.into_iter().map(|p| self.typed_type_def(p, mctx)).collect());
            tparams = syms;
            value_clauses.remove(0);
        }
        for clause in value_clauses {
            let (mut trees, mut params, mut kind) = (Vec::new(), Vec::new(), (false, false));
            for p in self.ast.items(clause).to_vec() {
                let Tree::ValDef { name, tpt, mods, .. } = *self.ast.tree(p) else {
                    return Err("polymorphic structural refinement");
                };
                let pflags = self.ast.modifiers(mods).flags;
                kind = (pflags.is_one_of(GIVEN.or(IMPLICIT)), pflags.is(GIVEN));
                if matches!(self.ast.tree(tpt), Tree::ByNameTypeTree { .. }) {
                    return Err("structural refinement with a by-name parameter");
                }
                let typed = self.typed_type(tpt, mctx);
                let tpe = self.tpd.tpe(typed);
                if matches!(self.u.ty(tpe), Type::Error) {
                    return Err("structural refinement of an unsupported type");
                }
                let ptext = self.text(name.name).to_owned();
                let psym = self.syms.define(
                    self.u,
                    &ptext,
                    Kind::Param,
                    sym,
                    pflags.or(f::TERM_PARAM).to_term_flags(),
                );
                self.u.set_info(psym, tpe);
                let level = self.levels.push(Some(mctx.level), Level::Scope(vec![(ptext.clone(), psym)]));
                mctx = Ctx { level, ..mctx };
                let pspan = self.ast.span(p);
                trees.push(self.build().val_def(psym, typed, None, pspan));
                params.push((self.u.intern_name(&ptext), tpe));
            }
            clause_trees.push(trees);
            clause_params.push((params, kind));
        }
        let result = self.typed_type(tpt, mctx);
        let mut info = self.tpd.tpe(result);
        if matches!(self.u.ty(info), Type::Error) {
            return Err("structural refinement of an unsupported type");
        }
        match clauses {
            Some(_) if clause_params.is_empty() && tparams.is_empty() => {
                info = self.u.intern_type(Type::ByName(info))
            }
            _ => {
                for (params, (implicit, contextual)) in clause_params.into_iter().rev() {
                    let binder = self.u.fresh_binder();
                    let method = Type::Method { binder, params, result: info, implicit, contextual };
                    info = self.u.intern_type(method);
                }
            }
        }
        if !tparams.is_empty() {
            let binder = self.syms.fresh_binder();
            let params: Vec<(NameId, TypeId)> = tparams
                .iter()
                .map(|&p| {
                    let bounds = self.u.info(p);
                    (self.u.interned_name(p), self.param_refs(bounds, &tparams, binder))
                })
                .collect();
            let result = self.param_refs(info, &tparams, binder);
            info = self.u.intern_type(Type::Poly { binder, params, result });
        }
        self.u.set_info(sym, info);
        let span = self.ast.span(r);
        let tree = match clauses {
            Some(_) => {
                let refs: Vec<&[TNodeId]> = clause_trees.iter().map(Vec::as_slice).collect();
                self.build().def_def(sym, &refs, result, None, span)
            }
            None => self.build().val_def(sym, result, None, span),
        };
        Ok((text, info, tree))
    }

    /// Completes the members of `t`'s class that the refinements of `pt` name, and for `t` or
    /// `pt` a type member `p.T`, the members `T` of the class of `p`.
    pub fn complete_refined_members(&mut self, t: TypeId, pt: TypeId) {
        let mut names = Vec::new();
        let mut r = pt;
        while let Type::Refined(parent, name, _) = self.u.ty(r).clone() {
            names.push(name);
            r = parent;
        }
        let class = names.first().and_then(|_| self.u.class_symbol(t));
        let mut wanted: Vec<(SymbolId, String)> =
            names.into_iter().filter_map(|n| Some((class?, self.u.name(n).to_owned()))).collect();
        for side in [t, pt] {
            if let Some((Some(p), s)) = self.u.sym_ref(side)
                && self.u.symbol(s).kind == Kind::Type
            {
                let widened = self.u.widen(p);
                if let Some(c) = self.u.class_symbol(widened) {
                    wanted.push((c, self.u.symbol(s).name.clone()));
                }
            }
        }
        for (class, name) in wanted {
            for m in self.u.member(class, &name) {
                self.complete(m);
            }
        }
    }
}
