// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//   compiler/src/dotty/tools/dotc/core/Annotations.scala
//   compiler/src/dotty/tools/dotc/cc/CaptureOps.scala
//! Capture-set annotations (`@retains`) in pickled types (dotty's `CompactAnnotation` case of
//! `pickleNewType`), and emptied (`CleanupRetains`).

use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree};
use crate::typer::capture_types::is_retains_like;
use crate::typer::seen_from::normalized;
use names::nme;
use rustc_hash::FxHashMap;
use tasty::tags::{ANNOTATEDtype, APPLY, NEW, SELECTin, TYPEAPPLY};
use tasty::write::Addr;
use tasty::{Binder, Kind, SymbolId, Type, TypeId, Universe};

impl TreePickler<'_> {
    /// Type `parent @annot` if `annot` is a capture-set annotation class type, as `parent` and
    /// the type `annot`, `retains[C]` of capture set `C`, or below source version 3.9 the tree
    /// `new retains[C]()` without positions (dotty's `oldTree`); `None` for another annotation.
    /// Refuses a capture set that refers to other than static definitions, but for one written in
    /// a typed pattern or `new` a definition of the file, and, in a kept synthetic tree or a
    /// super accessor, the `this` of its class.
    pub fn retaining(&mut self, parent: TypeId, annot: TypeId) -> Option<Result<(), Refusal>> {
        if !is_retains_like(self.u, annot) {
            return None;
        }
        let own = self.type_tree.and_then(|t| self.capture_classes.get(&t).copied()).or(self.accessor_class);
        let def = self.type_tree.and_then(|t| self.capture_defs.get(&t).copied());
        let annotated = self.u.intern_type(Type::Annotated(parent, annot));
        let written = self.typed_tpts.iter().flatten().any(|&t| self.tpd.get(t).tpe == Some(annotated));
        let (defined, here) = (&self.method_binders, written.then_some(&self.defs));
        if !is_static_capture_set(self.u, annot, (own, def), self.accessor_seen, defined, here) {
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
    /// Refuses such an info with a capture set that refers to other than static definitions and
    /// the `this` of `sym`'s class, as `b: B^{a}` of `class C(val a: A^, val b: B^{a})`.
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
        let mut captures = Vec::new();
        self.u.map_type(info, &mut |u, part| {
            if let Type::Annotated(_, annot) = *u.ty(part)
                && is_retains_like(u, annot)
            {
                captures.push(annot);
            }
            None
        });
        let class =
            std::iter::successors(Some(sym), |&s| self.u.symbol(s).owner).find(|&s| self.u.is_class(s));
        let defined = &self.method_binders;
        if captures.iter().any(|&c| !is_static_capture_set(self.u, c, (class, None), None, defined, None)) {
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

/// Whether capture-set annotation type `annot` refers to no `this`, parameter or definition other
/// than a static one, a parameter of definition `def` or of a method type whose parameters
/// `defined` has, a definition or class `this` among definitions `here` (those of the file for a
/// set written in a typed pattern or `new`: `case it: (It[E]^{it})` in `def from[E](it: It[E]^)`), the
/// `this` of class `own` or of a class enclosing it, which mapping a type that it annotates
/// leaves alone; of an enclosing class only if seeing it from `seen`, the prefix and class of a
/// super accessor's member, keeps it (`other.Views` of an inner trait `Views`).
fn is_static_capture_set(
    u: &mut Universe,
    annot: TypeId,
    (own, def): (Option<SymbolId>, Option<SymbolId>),
    seen: Option<(TypeId, SymbolId)>,
    defined: &FxHashMap<Binder, Vec<SymbolId>>,
    here: Option<&FxHashMap<SymbolId, Option<Addr>>>,
) -> bool {
    let local = |s: SymbolId| here.is_some_and(|h| h.contains_key(&s));
    let mut is_static = true;
    u.map_type(annot, &mut |u, part| {
        let this = matches!(u.ty(part), Type::This(_));
        is_static &= match *u.ty(part) {
            Type::This(c) => match *u.ty(c) {
                Type::Package(_) | Type::PackageName(_) => true,
                _ => u.class_symbol(c).is_some_and(|c| {
                    let mut enclosing = std::iter::successors(own, |&o| u.symbol(o).owner);
                    let kept =
                        |u: &mut Universe| seen.is_none_or(|(p, o)| u.as_seen_from(part, p, o) == part);
                    u.is_static_owner(c)
                        || Some(c) == own
                        || local(c)
                        || (enclosing.any(|o| o == c) && kept(u))
                }),
            },
            Type::Sym(_, s) => {
                u.is_static_symbol(s) || def.is_some_and(|d| u.symbol(s).owner == Some(d)) || local(s)
            }
            Type::Param(b, _) => defined.contains_key(&b),
            Type::Local(..) | Type::RecThis(_) | Type::Skolem(..) | Type::Var(_) => false,
            _ => true,
        };
        this.then_some(part)
    });
    is_static
}

/// Empties in `t` the capture set of each capture-checking annotation, `retains[Nothing]`, but
/// keeps each type annotated `retainsCap` as it is (dotty's `CleanupRetains` where capture
/// checking is enabled).
pub fn emptied(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, part| {
        let (inner, annot) = u.annotated(part)?;
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
