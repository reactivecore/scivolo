// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Wildcard capture of the type of a view's argument, an adapted tree or an overloaded
//! method's argument (dotty's `Inferencing.captureWildcards`).

use super::super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Captures the wildcards of `t`: `List[? <: Number]` becomes `List[x.CAP]` for a fresh
    /// skolem `x` of type `scala.runtime.TypeBox[Nothing, Number]`, each wildcard bounded also by
    /// its type parameter; `None` if `t` has no top-level wildcard argument.
    pub fn captured_wildcards(&mut self, t: TypeId) -> Option<TypeId> {
        captured(self, t, true)
    }

    /// Captures the wildcards of `t`, the type of an argument of an overload, as
    /// `captured_wildcards` does; `None` for a stable path, which dotty's `compareCaptured`
    /// captures by selecting the type parameters on it, and for a Java class such as
    /// `java.util.List[?]` whose parameter has no upper bound (dotty's `FromJavaObject`).
    pub fn captured_args(&mut self, t: TypeId) -> Option<TypeId> {
        let t = self.zonk(t);
        if self.u.sym_of(t).is_some() && super::super::stability::is_stable_path(self.u, t) {
            return None;
        }
        captured(self, t, false)
    }

    /// `tree` cast to its type with wildcards captured if that conforms to `pt` (dotty's
    /// `captureWildcardsCompat` in `adapt`); `None` for a stable path, as `captured_args`.
    pub fn captured_cast(&mut self, tree: TNodeId, pt: TypeId) -> Option<Result<TNodeId, &'static str>> {
        let t = self.zonk(self.tpd.tpe(tree));
        if self.u.sym_of(t).is_some() && super::super::stability::is_stable_path(self.u, t) {
            return None;
        }
        let captured = captured(self, t, true)?;
        self.conforms(captured, pt).then(|| self.cast_to_captured(tree, captured))
    }

    /// Argument `arg` of a by-name parameter, an error if its wildcards were captured (dotty's
    /// `checkNoWildcardCaptureForCBN`).
    pub fn by_name_uncaptured(&mut self, arg: TNodeId) -> TNodeId {
        let tpe = self.tpd.tpe(arg);
        let Some(args) = self.u.applied(tpe).map(|(_, args)| args.to_vec()) else { return arg };
        if !args.iter().any(|&a| self.u.sym_of(a).is_some()) {
            return arg;
        }
        let cap =
            self.resolve_full_name("scala.runtime.TypeBox", true).and_then(|b| self.u.decl(b, "CAP").ok());
        let is_cap =
            |t: &Self, a: TypeId| t.u.sym_of(a).is_some_and(|s| cap.as_ref().is_some_and(|c| c.contains(&s)));
        if !args.iter().any(|&a| is_cap(self, a)) {
            return arg;
        }
        let uncast = match *self.tpd.tree(arg) {
            T::TypeApply { fun, .. } => match *self.tpd.tree(fun) {
                T::Select { qualifier, .. } if self.tpd.span(arg).is_synthetic() => qualifier,
                _ => arg,
            },
            _ => arg,
        };
        let shown = self.u.show_user(self.tpd.tpe(uncast));
        let span = self.tpd.span(arg);
        let msg = format!(
            "argument for by-name parameter is not a value\nand contains wildcard arguments: {shown}\n\n\
             Assign it to a val and pass that instead.\n"
        );
        self.error_at(span, msg);
        self.error_tree(span)
    }

    /// `tree.$asInstanceOf$[to]`, at `tree`'s span made synthetic, `to` at its end.
    pub fn cast_to_captured(&mut self, tree: TNodeId, to: TypeId) -> Result<TNodeId, &'static str> {
        let span = self.tpd.span(tree).to_synthetic();
        let any = self.u.defn().any;
        let found = self.u.decl(any, names::nme::AS_INSTANCE_OF_PM).ok().and_then(|d| d.first().copied());
        let fun = self.build().select(tree, found.ok_or("Any.$asInstanceOf$")?, span);
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, span.end_pos(), Some(to), None);
        Ok(self.build().type_apply(fun, &[tpt], to, span))
    }
}

/// Captures the wildcards of `tpe` as `captured_wildcards` does, also under a flexible,
/// annotated or refined type and in both sides of `&` and `|`; unless `java_unbounded`, gives
/// `None` also for a Java class whose parameter has no upper bound.
fn captured(t: &mut Typer<'_>, tpe: TypeId, java_unbounded: bool) -> Option<TypeId> {
    let tpe = t.zonk(tpe);
    let tpe = t.u.widen(tpe);
    let tpe = t.u.dealias(tpe);
    match *t.u.ty(tpe) {
        Type::Flexible(hi) => {
            let hi = captured(t, hi, java_unbounded)?;
            return Some(t.u.flexible(hi));
        }
        Type::Annotated(p, annot) => {
            let p = captured(t, p, java_unbounded)?;
            return Some(t.u.intern_type(Type::Annotated(p, annot)));
        }
        Type::Refined(p, name, info) => {
            let p = captured(t, p, java_unbounded)?;
            return Some(t.u.intern_type(Type::Refined(p, name, info)));
        }
        Type::And(a, b) => {
            let [a, b] = captured_sides(t, [a, b], java_unbounded)?;
            return Some(t.u.intern_type(Type::And(a, b)));
        }
        Type::Or(a, b, soft) => {
            let [a, b] = captured_sides(t, [a, b], java_unbounded)?;
            return Some(t.u.intern_type(Type::Or(a, b, soft)));
        }
        _ => {}
    }
    let (tycon, args) = t.u.applied(tpe).map(|(tycon, args)| (tycon, args.to_vec()))?;
    if !args.iter().any(|&a| t.u.bounds(a).is_some()) {
        return None;
    }
    let params = t.u.class_symbol(tycon).map(|c| t.u.type_params(c)).unwrap_or_default();
    if params.len() != args.len() {
        return None;
    }
    let type_box = t.resolve_full_name("scala.runtime.TypeBox", true)?;
    let cap = t.u.decl(type_box, "CAP").ok()?.first().copied()?;
    let java = !java_unbounded && t.u.class_symbol(tycon).is_some_and(|c| t.is_java_class_name(c));
    let mut captured = Vec::new();
    for (a, &p) in args.into_iter().zip(&params) {
        let a = t.with_param_bounds(a, Some(p), &params);
        let Some((lo, hi)) = t.u.bounds(a) else {
            captured.push(a);
            continue;
        };
        let from_java_object = t.u.defn().from_java_object;
        let java_any = hi == tasty::ANY || t.u.sym_of(hi) == Some(from_java_object);
        if java && java_any {
            return None;
        }
        let boxed = crate::defn::applied(t.u, type_box, vec![lo, hi]);
        let skolem = t.u.fresh_skolem(boxed);
        captured.push(t.u.intern_type(Type::Sym(Some(skolem), cap)));
    }
    Some(t.u.intern_applied(tycon, &captured))
}

/// Captures the wildcards of both sides of `&` or `|`, a side without any kept; `None` if
/// neither has one.
fn captured_sides(t: &mut Typer<'_>, sides: [TypeId; 2], java_unbounded: bool) -> Option<[TypeId; 2]> {
    let [a, b] = sides.map(|s| captured(t, s, java_unbounded));
    (a.is_some() || b.is_some()).then(|| [a.unwrap_or(sides[0]), b.unwrap_or(sides[1])])
}
