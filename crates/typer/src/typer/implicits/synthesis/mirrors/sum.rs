// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! Sum mirrors of sealed classes and enums (dotty's `sumMirror`): the companion or an anonymous
//! class, cast to the mirror type.

use super::child_prefixes::child_prefix;
use super::children::child_type;
use super::formal::MirrorCore;
use super::sources::MirrorSource;
use super::{MirrorSynthesis, label_pairs};
use crate::defn::MIRROR_SUM;
use crate::tpd::TNodeId;
use crate::typer::Typer;
use crate::typer::paths::MAX_PATH_DEPTH;
use tasty::tags::CASE;
use tasty::{SymbolId, Type, TypeId, Universe};

impl MirrorSynthesis {
    /// Synthesizes the mirror of a sealed trait, sealed abstract class or enum whose children
    /// all have mirrors; `None` for another type.
    pub fn sum(self, t: &mut Typer<'_>, source: MirrorSource) -> Result<Option<TNodeId>, &'static str> {
        let MirrorSource::Class(pre, cls) = source else { return Ok(None) };
        let companion = sum_companion(t, cls);
        if !self.is_generic_sum(t, cls, companion.is_some(), pre)? {
            return Ok(None);
        }

        let owner = t.u.symbol(cls).owner;
        let path = pre.filter(|&p| !is_plain_prefix(t.u, p) || is_inheriting_module(t.u, p, owner));
        let children = t.sealed_children(cls)?;
        let types = self.child_types(t, (path, cls), &children)?;
        let labels: Vec<String> = children.iter().map(|&c| t.u.symbol(c).name.clone()).collect();
        let nested = t.nested_pairs(&types)?;
        let (mono, elem_types) = self.formal.mono_and_elems(t.u, nested);
        let elem_labels = label_pairs(t, &labels)?;
        let label = t.u.symbol(cls).name.clone();
        let core =
            MirrorCore { parent: MIRROR_SUM, mono, label: &label, elems: Some((elem_types, elem_labels)) };
        let cast = self.mirror_type(t, core)?;

        let mirror = match companion {
            Some(module) => {
                let path = t.u.intern_type(Type::Sym(pre, module));
                t.path_tree(path, self.at, MAX_PATH_DEPTH).ok_or("Mirror companion outside the subset")?
            }
            None => self.anonymous(t, mono, true)?,
        };
        self.cast(t, mirror, cast)
    }

    /// The types of `children` of sealed class `cls` as elements of its mirror, each selected
    /// from `path` if the class is.
    fn child_types(
        self,
        t: &mut Typer<'_>,
        (path, cls): (Option<TypeId>, SymbolId),
        children: &[SymbolId],
    ) -> Result<Vec<TypeId>, &'static str> {
        let mirrored = self.formal.mirrored;
        let target = match *t.u.types().get(mirrored) {
            Type::Lambda { result, .. } => result,
            _ => mirrored,
        };
        let mut types = Vec::new();
        for &c in children {
            let prefix = match path {
                Some(p) => {
                    let prefix = child_prefix(t.u, p, cls, c);
                    Some(prefix.ok_or("sum Mirror of a class selected from a path")?)
                }
                None => None,
            };
            types.push(child_type(t, c, target, prefix)?);
        }
        Ok(types)
    }
}

/// The companion of sealed class `c` if it is its sum mirror (dotty's `useCompanionAsSumMirror`):
/// for a class of the library, one extending `Mirror.Sum`.
fn sum_companion(t: &mut Typer<'_>, c: SymbolId) -> Option<SymbolId> {
    let module = t.u.companion_module(c)?;
    let module_class = t.u.module_class(module)?;
    if t.u.symbol(module_class).flags.has(CASE) {
        return None;
    }
    if t.syms.def_trees.contains_key(&c) {
        return Some(module);
    }
    let sum = t.resolve_full_name(MIRROR_SUM, true)?;
    t.u.derives_from(module_class, sum).then_some(module)
}

/// Whether `p` refers to an object whose class is not `owner`, or is the `this` of a subclass of
/// class `owner`, which it inherits from.
fn is_inheriting_module(u: &mut Universe, p: TypeId, owner: Option<SymbolId>) -> bool {
    match *u.types().get(p) {
        Type::Sym(_, s) => u.module_class(s) != owner,
        Type::This(c) => match (u.class_symbol(c), owner) {
            (Some(c), Some(o)) => c != o && u.is_class(o) && u.derives_from(c, o),
            _ => false,
        },
        _ => false,
    }
}

/// Whether prefix `p` is a package or the `this` of a class, under which the children's types are
/// their own references.
fn is_plain_prefix(u: &mut Universe, p: TypeId) -> bool {
    match *u.types().get(p) {
        Type::Package(_) | Type::PackageName(_) | Type::This(_) | Type::NoPrefix => true,
        Type::Sym(p, s) => u.is_module(s) && p.is_none_or(|p| is_plain_prefix(u, p)),
        _ => false,
    }
}
