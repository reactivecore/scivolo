// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The annotations of definitions typed as `new` expressions once their package's statements
//! are typed (dotty's `annotContext`, `typedAnnotation`, `Checking.checkAnnotClass`).

use super::annotations::STATIC;
use crate::ctx::{Ctx, Level};
use crate::tpd::TNodeId;
use crate::typer::{Pt, Typer};
use rustc_hash::FxHashMap;
use syntax::ast::NodeId;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// The context of the annotations of `sym`, defined in `ctx` (dotty's `annotContext`): for a
    /// parameter the context outside its method, with the method's type parameters only; owned by
    /// a local dummy of `sym`'s owner where that context's owner is a class or package.
    pub fn annot_ctx(&mut self, sym: SymbolId, ctx: Ctx) -> Ctx {
        let Some(owner) = self.u.symbol(sym).owner else { return ctx };
        let is_param = matches!(self.u.symbol(sym).kind, Kind::Param | Kind::TypeParam) && ctx.owner == owner;
        let (outer, tparams) = match self.levels.outer(ctx.level).filter(|_| is_param) {
            Some(level) => {
                let tparams: Vec<(String, SymbolId)> = (self.levels.scope_symbols(ctx.level).into_iter())
                    .filter(|&p| self.u.is_type_symbol(p))
                    .map(|p| (self.u.symbol(p).name.clone(), p))
                    .collect();
                let outer_owner = self.u.symbol(owner).owner.unwrap_or(owner);
                (Ctx { owner: outer_owner, level, ..ctx }, tparams)
            }
            None => (ctx, Vec::new()),
        };
        let in_members = matches!(self.u.symbol(outer.owner).kind, Kind::Class | Kind::Package);
        if tparams.is_empty() && !in_members {
            return outer;
        }
        let dummy = self.local_dummy(owner);
        let level = if tparams.is_empty() {
            outer.level
        } else {
            self.levels.push(Some(outer.level), Level::Scope(tparams))
        };
        Ctx { owner: dummy, level, ..outer }
    }

    /// Types the annotations of the unit's definitions not typed yet in their contexts, an
    /// annotation two definitions share once, in the context of the first one queued, and keeps
    /// them on their symbols.
    pub fn typed_annotations(&mut self) {
        let mut typed_trees: FxHashMap<NodeId, Option<TNodeId>> = FxHashMap::default();
        let mut i = 0;
        while let Some(&(sym, annot, ctx)) = self.syms.annotation_trees.get(i) {
            i += 1;
            let typed = match typed_trees.get(&annot) {
                Some(&t) => t,
                None => {
                    let typed = self.typed(annot, Pt::Any, ctx);
                    let t = (!self.is_error(typed) && self.check_annot_class(annot, typed)).then_some(typed);
                    *typed_trees.entry(annot).or_insert(t)
                }
            };
            let Some(typed) = typed else { continue };
            let tpe = self.tpd.tpe(typed);
            if self.u.class_symbol(tpe).is_some_and(|c| self.u.full_name(c) == STATIC) {
                self.check_static(sym, annot);
            }
            self.syms.annotations.entry(sym).or_default().push(typed);
        }
        self.syms.annotation_trees.clear();
    }

    /// Whether typed annotation `typed` of tree `annot` is of a class that extends
    /// `scala.annotation.Annotation`; reports it if not.
    fn check_annot_class(&mut self, annot: NodeId, typed: TNodeId) -> bool {
        let tpe = self.tpd.tpe(typed);
        if self.derives_from_named(tpe, "scala.annotation.Annotation") {
            return true;
        }
        let name = self.u.class_symbol(tpe).map(|c| self.u.symbol(c).name.clone()).unwrap_or_default();
        let msg = format!(
            "class {name} is not a valid Scala annotation: it does not extend `scala.annotation.Annotation`"
        );
        self.error(annot, msg);
        false
    }
}
