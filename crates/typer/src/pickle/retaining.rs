// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//   compiler/src/dotty/tools/dotc/core/Annotations.scala
//   compiler/src/dotty/tools/dotc/cc/CaptureOps.scala
//! Capture-set annotations (`@retains`) in pickled types (dotty's `CompactAnnotation` case of
//! `pickleNewType`), and emptied (`CleanupRetains`).

use super::capture_refs::{CaptureScope, is_static_capture_set, sets_on_fields};
use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree};
use crate::typer::capture_types::is_retains_like;
use crate::typer::seen_from::normalized;
use names::nme;
use tasty::defn::DECLARED;
use tasty::tags::{ANNOTATEDtype, APPLY, NEW, SELECTin, TYPEAPPLY};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

impl TreePickler<'_> {
    /// Type `parent @annot` if `annot` is a capture-set annotation class type, as `parent` and
    /// the type `annot`, `retains[C]` of capture set `C`, or below source version 3.9 the tree
    /// `new retains[C]()` without positions (dotty's `oldTree`); `None` for another annotation.
    /// Refuses a capture set that refers to other than static definitions and those enclosing the
    /// tree pickled, in a super accessor only to the `this` of its class.
    pub fn retaining(&mut self, parent: TypeId, annot: TypeId) -> Option<Result<(), Refusal>> {
        if !is_retains_like(self.u, annot) {
            return None;
        }
        let own = self.type_tree.and_then(|t| self.capture_classes.get(&t).copied()).or(self.accessor_class);
        let def = self.type_tree.and_then(|t| self.capture_defs.get(&t).copied());
        let scope = CaptureScope {
            own,
            def,
            seen: self.accessor_seen,
            methods: &self.method_binders,
            binders: &self.open,
            open: &self.open_defs,
        };
        if !is_static_capture_set(self.u, annot, &scope) {
            return Some(Err(
                "an annotated type of a capture set that refers to other than static definitions",
            ));
        }
        let cap = self.u.class_symbol(annot).is_some_and(|c| self.u.symbol(c).name == nme::RETAINS_CAP);
        if !cap && self.u.applied(annot).is_none() {
            return Some(Err("a capture set a library annotation tree holds"));
        }
        let annot = class_by_symbol(self.u, annot);
        Some(self.with_length(ANNOTATEDtype, &mut |p| {
            p.ty(parent, false)?;
            match p.compact_annotations {
                true => p.ty(annot, false),
                false => p.old_retains(annot),
            }
        }))
    }

    /// Annotation tree `annot` of a definition; of a capture-set annotation class in a file, its
    /// type or, below source version 3.9, `new C[Ts]()` of it (dotty's `CompactAnnotation`).
    pub fn definition_annotation(&mut self, annot: TNodeId) -> Result<(), Refusal> {
        let tpe = self.tpd.tpe(annot);
        if self.file.is_none() || !is_retains_like(self.u, tpe) {
            return self.tree(annot);
        }
        let tpe = class_by_symbol(self.u, tpe);
        match self.compact_annotations {
            true => self.ty(tpe, false),
            false => self.old_retains(tpe),
        }
    }

    /// `new C[Ts]()` of capture-set annotation type `annot`, `C[Ts]` or `C`, through the class's
    /// primary constructor (dotty's `New(tpe, Nil)`).
    fn old_retains(&mut self, annot: TypeId) -> Result<(), Refusal> {
        let (tycon, args) = match self.u.applied(annot) {
            Some((tycon, args)) => (tycon, args.to_vec()),
            None => (annot, Vec::new()),
        };
        let class = self.u.class_symbol(tycon).ok_or("a capture-set annotation of no class")?;
        let init = self.u.decls_named(class, nme::CONSTRUCTOR).into_iter().next();
        let init = init.ok_or("a capture-set annotation without a constructor")?;
        let select = |p: &mut Self| {
            p.with_length(SELECTin, &mut |p| {
                p.signed_name(init, true)?;
                p.byte(NEW);
                p.ty(tycon, false)?;
                p.ty(tycon, false)
            })
        };
        self.with_length(APPLY, &mut |p| match args.is_empty() {
            true => select(p),
            false => p.with_length(TYPEAPPLY, &mut |p| {
                select(p)?;
                args.iter().try_for_each(|&a| p.ty(a, false))
            }),
        })
    }

    /// Records the info of value or parameter `sym` as the type of its type tree `tpt` if the
    /// tree is synthetic, as a class parameter's field, and has the info's type without its
    /// capture-set annotations: `label: Label[T]^{}` of `class Break[T](val label: Label[T]^{})`.
    /// A field's capture sets name the class's fields for its constructor's parameters, as
    /// `b: B^{C.this.a}` of `class C(val a: A^, val b: B^{a})`. Refuses such an info with a capture
    /// set that refers to other than static definitions and members and `this` of `sym`'s class.
    pub fn keep_captures(&mut self, sym: SymbolId, tpt: TNodeId) -> Result<(), Refusal> {
        let (Tree::TypeTree { inferred: false }, Some(tpe)) = (self.tpd.tree(tpt), self.tpd.get(tpt).tpe)
        else {
            return Ok(());
        };
        if !matches!(self.u.symbol(sym).kind, Kind::Val | Kind::Param) {
            return Ok(());
        }
        let info = self.u.info(sym);
        if normalized(self.u, info) != tpe {
            return Ok(());
        }
        let class =
            std::iter::successors(Some(sym), |&s| self.u.symbol(s).owner).find(|&s| self.u.is_class(s));
        let info = match class.filter(|&c| self.u.symbol(sym).owner == Some(c)) {
            Some(c) => sets_on_fields(self.u, info, c),
            None => info,
        };
        let mut captures = Vec::new();
        self.u.map_type(info, &mut |u, part| {
            if let Type::Annotated(_, annot) = *u.ty(part)
                && is_retains_like(u, annot)
            {
                captures.push(annot);
            }
            None
        });
        let enclosing: Vec<SymbolId> = std::iter::successors(class, |&c| self.u.symbol(c).owner).collect();
        let scope = CaptureScope {
            own: class,
            def: None,
            seen: None,
            methods: &self.method_binders,
            binders: &self.open,
            open: &enclosing,
        };
        if captures.iter().any(|&c| !is_static_capture_set(self.u, c, &scope)) {
            return Err(
                "a synthetic type tree of a capture set that refers to other than static definitions",
            );
        }
        if !captures.is_empty() {
            self.kept_captures.insert(tpt, info);
            if let Some(class) = class {
                self.capture_classes.insert(tpt, class);
            }
        }
        Ok(())
    }
}

/// Capture-set annotation type `annot` with its class referred to by symbol, as a library's
/// `retains` read by name (dotty's `annotClass(tree).typeRef` of a `CompactAnnotation`).
fn class_by_symbol(u: &mut Universe, annot: TypeId) -> TypeId {
    let Some(class) = u.class_symbol(annot) else { return annot };
    let class_ref = type_ref(u, class);
    match u.applied(annot).map(|(_, args)| args.to_vec()) {
        Some(args) => u.intern_applied(class_ref, &args),
        None => class_ref,
    }
}

/// Empties in `t` the capture set of each capture-checking annotation, `retains[Nothing]`, but
/// keeps each type annotated `retainsCap` or `@caps.internal.declared` as it is (dotty's
/// `CleanupRetains` where capture checking is enabled).
pub fn emptied(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, part| {
        let (inner, annot) = u.annotated(part)?;
        if u.is_class_named(annot, DECLARED) {
            return Some(part);
        }
        if !is_retains_like(u, annot) {
            return None;
        }
        if u.class_symbol(annot).is_some_and(|c| u.symbol(c).name == nme::RETAINS_CAP) {
            return Some(part);
        }
        let inner = emptied(u, inner);
        let annot = match u.applied(annot) {
            Some((tycon, _)) => u.intern_applied(tycon, &[tasty::NOTHING]),
            None => u.intern_applied(annot, &[tasty::NOTHING]),
        };
        Some(u.intern_type(Type::Annotated(inner, annot)))
    })
}
