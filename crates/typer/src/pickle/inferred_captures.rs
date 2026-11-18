// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//   compiler/src/dotty/tools/dotc/cc/CaptureOps.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Capture sets in inferred type trees of capture-checked files, as the typer gives them (ADR
//! 0126): each emptied (dotty's `CleanupRetains` of an inferred `TypeTree` in PostTyper), an
//! overriding member's kept (`explicifyTpt`).

use super::TreePickler;
use super::retaining::emptied;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree};
use crate::typer::capture_types::has_captures;
use tasty::tags::{INLINE, PRIVATE};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

impl TreePickler<'_> {
    /// Whether the file being pickled is capture-checked.
    pub fn capture_checked(&self) -> bool {
        self.file.is_some_and(|f| f.attributes.features.capture_checking())
    }

    /// Records the type inferred type tree `tpt` of type `tpe` of a capture-checked file is
    /// pickled with: `tpe` with each capture set but `retainsCap`'s empty, unless recorded before.
    pub fn empty_inferred_captures(&mut self, tpt: TNodeId, tpe: TypeId) {
        if !self.capture_checked()
            || !matches!(self.tpd.tree(tpt), Tree::TypeTree { inferred: true })
            || self.kept_captures.contains_key(&tpt)
            || !has_captures(self.u, tpe)
        {
            return;
        }
        let kept = emptied(self.u, tpe);
        self.kept_captures.insert(tpt, kept);
    }

    /// Records in a capture-checked file the inferred type trees among type arguments `args` of
    /// inline method `fun` with `@caps.internal.inferred` around their emptied types (dotty's
    /// `PostTyper.markInferred`): `x.nn` is `x.nn[It[A]^{} @inferred]`.
    pub fn mark_inferred_type_args(&mut self, fun: TNodeId, args: &[TNodeId]) {
        let inline = self.tpd.sym(fun).is_some_and(|f| self.u.symbol(f).flags.has(INLINE));
        if !inline || !self.capture_checked() {
            return;
        }
        let Some(mark) = caps_internal_class(self.u, "inferred") else { return };
        for &arg in args {
            let arg = match *self.tpd.tree(arg) {
                Tree::NamedArg { arg, .. } => arg,
                _ => arg,
            };
            if let (Tree::TypeTree { inferred: true }, Some(tpe)) =
                (self.tpd.tree(arg), self.tpd.get(arg).tpe)
            {
                let cleaned = emptied(self.u, tpe);
                let marked = self.u.intern_type(Type::Annotated(cleaned, mark));
                self.kept_captures.insert(arg, marked);
            }
        }
    }

    /// Type `t` of a pattern variable's info or an extractor pattern, with each capture set but
    /// `retainsCap`'s empty in a capture-checked file (dotty's `CleanupRetains` of the `Bind`
    /// and `UnApply` cases of PostTyper).
    pub fn cleaned_pattern_type(&mut self, t: TypeId) -> TypeId {
        match self.capture_checked() && has_captures(self.u, t) {
            true => emptied(self.u, t),
            false => t,
        }
    }

    /// Records the type of inferred type tree `tpt` of member `sym` of a capture-checked file
    /// as it is, capture sets not emptied, if `sym` overrides a member (dotty's `explicifyTpt`
    /// makes the tree not inferred); its sets may name `sym`'s parameters.
    pub fn keep_overriding_captures(&mut self, sym: SymbolId, tpt: TNodeId) {
        if !self.capture_checked() || overridden_infos(self.u, sym).is_empty() {
            return;
        }
        let (Tree::TypeTree { inferred: true }, Some(tpe)) = (self.tpd.tree(tpt), self.tpd.get(tpt).tpe)
        else {
            return;
        };
        if !has_captures(self.u, tpe) {
            return;
        }
        self.kept_captures.insert(tpt, tpe);
        self.capture_defs.insert(tpt, sym);
        if let Some(class) = self.u.symbol(sym).owner.filter(|&o| self.u.is_class(o)) {
            self.capture_classes.insert(tpt, class);
        }
    }
}

/// The class type of annotation `name` of `scala.caps.internal`.
pub fn caps_internal_class(u: &mut Universe, name: &str) -> Option<TypeId> {
    let caps = u.package("scala.caps")?;
    let internal = u.decl(caps, "internal$").ok()?.into_iter().find(|&d| u.is_class(d))?;
    let class = u.decls_named(internal, name).into_iter().find(|&d| u.is_class(d))?;
    Some(type_ref(u, class))
}

/// The infos, as seen from its class, of the members that member `sym` overrides: the non-private
/// term members of its name of its class's base classes (dotty's `allOverriddenSymbols`).
pub fn overridden_infos(u: &mut Universe, sym: SymbolId) -> Vec<TypeId> {
    let Some(cls) = u.symbol(sym).owner.filter(|&o| u.symbol(o).kind == Kind::Class) else {
        return Vec::new();
    };
    let this = u.this_type(cls);
    let name = u.symbol(sym).name.clone();
    let mut infos = Vec::new();
    for base in u.linearization(cls).into_iter().skip(1) {
        let members: Vec<SymbolId> = (u.symbol(base).decls.iter().copied())
            .filter(|&m| u.symbol(m).name == name && u.symbol(m).kind.is_term())
            .filter(|&m| !u.symbol(m).flags.has(PRIVATE))
            .collect();
        for m in members {
            infos.push(u.member_info(this, m));
        }
    }
    infos
}
