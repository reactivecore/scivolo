// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! Sum mirrors of sealed classes and enums (dotty's `sumMirror`): the companion or an anonymous
//! class, cast to the mirror type.

use super::super::Typer;
use super::mirror_sources::MirrorSource;
use super::mirror_types::{MirrorCore, MirrorFormal};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::tags::CASE;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn sum_mirror(
        &mut self,
        formal: &MirrorFormal,
        source: MirrorSource,
        at: Span,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, &'static str> {
        let MirrorSource::Class(pre, cls) = source else { return Ok(None) };
        let companion = self.sum_companion(cls);
        if !self.is_generic_sum(cls, companion.is_some(), ctx)? {
            return Ok(None);
        }
        if pre.is_some_and(|p| !self.is_plain_prefix(p)) {
            return Err("sum Mirror of a class selected from a path");
        }
        let children = self.sealed_children(cls)?;
        let target = match self.u.types().get(formal.mirrored).clone() {
            Type::Lambda { result, .. } => result,
            _ => formal.mirrored,
        };
        let mut types = Vec::new();
        for &c in &children {
            types.push(self.child_type(c, target)?);
        }
        let labels: Vec<String> = children.iter().map(|&c| self.u.symbol(c).name.clone()).collect();
        let nested = self.nested_pairs(&types)?;
        let (mono, elem_types) = self.mono_and_elems(formal.mirrored, nested);
        let elem_labels = self.label_pairs(&labels)?;
        let label = self.u.symbol(cls).name.clone();
        let core = MirrorCore {
            parent: "scala.deriving.Mirror$.Sum",
            mono,
            label: &label,
            elems: Some((elem_types, elem_labels)),
        };
        let cast = self.mirror_type(formal, core)?;
        let mirror = match companion {
            Some(module) => {
                let path = self.u.intern_type(Type::Sym(pre, module));
                self.path_tree(path, at, 8).ok_or("Mirror companion outside the subset")?
            }
            None => self.anonymous_mirror(mono, true, at, ctx)?,
        };
        self.cast_to(mirror, cast, at).map(Some)
    }

    /// The companion of sealed class `c` if it is its sum mirror (dotty's
    /// `useCompanionAsSumMirror`, for a class of the unit).
    fn sum_companion(&mut self, c: SymbolId) -> Option<SymbolId> {
        let module = self.u.companion_module(c)?;
        let case = self.u.module_class(module).is_some_and(|m| self.u.symbol(m).flags.has(CASE));
        (!case && self.syms.def_trees.contains_key(&c)).then_some(module)
    }

    /// Whether prefix `p` is a package or the `this` of a class, under which the children's
    /// types are their own references.
    fn is_plain_prefix(&mut self, p: TypeId) -> bool {
        match self.u.types().get(p).clone() {
            Type::Package(_) | Type::PackageName(_) | Type::This(_) | Type::NoPrefix => true,
            Type::Sym(p, s) => self.u.is_module(s) && p.is_none_or(|p| self.is_plain_prefix(p)),
            _ => false,
        }
    }
}
