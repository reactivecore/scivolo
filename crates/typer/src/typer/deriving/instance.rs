// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Deriving.scala
//! The definition of one derived instance (dotty's `processDerivedInstance` and
//! `typeclassInstance`).

use super::super::Typer;
use super::super::paths::MAX_PATH_DEPTH;
use super::kinds::Instance;
use super::trees::{node, tc_applied};
use super::{DerivedInstance, Deriver, Problem};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

/// How many aliases and type lambdas the class reference under a type class may lie below.
const CLASS_REF_DEPTH: u32 = 16;

/// The type class of a `derives` clause entry, its aliases expanded.
struct TypeClass {
    /// The class reference under the written type.
    tpe: TypeId,
    /// The prefix of that reference, also the prefix of the type class's companion.
    prefix: Option<TypeId>,
    cls: SymbolId,
}

impl Deriver {
    /// Makes the definition of the instance for type class tree `d`, as
    /// `given derived$Eq: Eq[C] = Eq.derived` for `derives Eq`; `None`, reported, if `d` is no
    /// class type, derived twice, or of a shape the class cannot be unified with.
    pub fn instance(&mut self, t: &mut Typer<'_>, d: NodeId) -> Option<DerivedInstance> {
        let typed = t.typed_type(d, self.ctx);
        let original = t.tpd.tpe(typed);
        if matches!(t.u.ty(original), Type::Error) {
            return None;
        }
        let Some(tc_type) = underlying_class_ref(t, original, CLASS_REF_DEPTH) else {
            let shown = t.types().show_type(original);
            t.error(d, format!("{shown} is not a class type"));
            return None;
        };
        let Some((prefix, cls)) = t.u.sym_ref(tc_type) else { unreachable!("a class reference") };
        let Some(written) = t.u.sym_of(original) else { unreachable!("a type reference") };
        let tc = TypeClass { tpe: tc_type, prefix, cls };

        let name = format!("derived${}", t.u.symbol(written).name);
        if !t.class_members(self.ctx.owner, &name).is_empty() || self.names.contains(&name) {
            let tc_name = t.u.symbol(written).name.clone();
            t.error(d, format!("duplicate type class derivation for {tc_name}"));
            return None;
        }
        self.names.push(name.clone());

        let span = Span::at(t.ast.span(d).point()).to_synthetic();
        let instance = self.shape(t, d, tc.cls, span)?;
        let (rhs, problem) = derived_rhs(t, d, &tc, span)?;
        let def = instance_def(t, &tc, &name, instance, rhs, span);
        Some(DerivedInstance { def, problem })
    }
}

/// `TC.derived` for type class `tc`, with the reason it cannot be called if its companion is
/// missing or its `derived` takes explicit parameters; `None`, reported, if the companion has
/// no stable path.
fn derived_rhs(
    t: &mut Typer<'_>,
    d: NodeId,
    tc: &TypeClass,
    span: Span,
) -> Option<(NodeId, Option<Problem>)> {
    let tc_name = t.u.symbol(tc.cls).name.clone();
    let Some(module) = t.u.companion_module(tc.cls) else {
        let kind = if t.u.is_trait(tc.cls) { "trait" } else { "class" };
        let rhs = node(t, Tree::TypeTree, span);
        return Some((rhs, Some(Problem::NoCompanion(format!("{kind} {tc_name}")))));
    };

    let module_ref = t.u.intern_type(Type::Sym(tc.prefix, module));
    let Some(qualifier) = t.path_tree(module_ref, span, MAX_PATH_DEPTH) else {
        t.unsupported(d, "derives clause of a type class without a stable path");
        return None;
    };
    let explicit = derived_takes_explicit_params(t, module);
    let shown = ref_text(t, module_ref);
    let qualifier = t.spliced(qualifier);
    let derived = TName { name: t.name("derived"), is_type: false };
    let rhs = node(t, Tree::Select { qualifier, name: derived }, span);
    Some((rhs, explicit.then_some(Problem::ExplicitParams(shown))))
}

/// `given name: TC[args] = rhs`, or a `given def` over the instance's type parameters and
/// evidence parameters; its type lambda recorded.
fn instance_def(
    t: &mut Typer<'_>,
    tc: &TypeClass,
    name: &str,
    instance: Instance,
    rhs: NodeId,
    span: Span,
) -> NodeId {
    let tpt = tc_applied(t, tc.tpe, instance.args, span);
    let name = TName { name: t.name(name), is_type: false };
    let evidence: Vec<NodeId> = instance
        .evidence
        .into_iter()
        .enumerate()
        .map(|(i, args)| {
            let tpt = tc_applied(t, tc.tpe, args, span);
            let name = TName { name: t.name(&format!("x${i}")), is_type: false };
            let mods = t.ast.mods(Modifiers::with_flags(f::TERM_PARAM.or(f::IMPLICIT)));
            node(t, Tree::ValDef { name, tpt, rhs: None, mods }, span)
        })
        .collect();

    let def = if instance.tparams.is_empty() && evidence.is_empty() {
        let mods = t.ast.mods(Modifiers::with_flags(f::GIVEN.or(f::LAZY)));
        node(t, Tree::ValDef { name, tpt, rhs: Some(rhs), mods }, span)
    } else {
        let clauses: Vec<_> =
            [instance.tparams, evidence].iter().filter(|c| !c.is_empty()).map(|c| t.ast.list(c)).collect();
        let paramss = t.ast.clauses(&clauses);
        let mods = t.ast.mods(Modifiers::with_flags(f::GIVEN));
        node(t, Tree::DefDef { name, paramss, tpt, rhs: Some(rhs), mods }, span)
    };
    if let Some(lambda) = instance.lambda {
        t.derivations.lambdas.insert(lambda, def);
    }
    def
}

/// The text of path type `p` as a reference to it shows it: `a.b.C`.
fn ref_text(t: &mut Typer<'_>, p: TypeId) -> String {
    match *t.u.ty(p) {
        Type::Sym(Some(q), s) if t.u.sym_of(q).is_some_and(|o| t.u.symbol(o).kind.is_term()) => {
            format!("{}.{}", ref_text(t, q), t.u.symbol(s).name)
        }
        Type::Sym(_, s) => t.u.symbol(s).name.clone(),
        _ => t.types().show_type(p),
    }
}

/// Whether the only `derived` member of type class companion `module` takes explicit term
/// parameters (dotty's `hasExplicitParams`).
fn derived_takes_explicit_params(t: &mut Typer<'_>, module: SymbolId) -> bool {
    let Some(class) = t.u.module_class(module) else { return false };
    let [m] = t.u.member(class, "derived")[..] else { return false };
    t.complete(m);
    let mut info = t.u.info(m);
    loop {
        match *t.u.ty(info) {
            Type::Poly { result, .. } => info = result,
            Type::Method { implicit: false, ref params, .. } if !params.is_empty() => return true,
            Type::Method { result, .. } => info = result,
            _ => return false,
        }
    }
}

/// The class reference under type `tpe`, also of a higher-kinded alias, at most `depth` aliases
/// and lambdas deep (dotty's `underlyingClassRef`).
fn underlying_class_ref(t: &mut Typer<'_>, tpe: TypeId, depth: u32) -> Option<TypeId> {
    let depth = depth.checked_sub(1)?;
    match *t.u.ty(tpe) {
        Type::Sym(_, s) if t.u.symbol(s).kind == Kind::Class => Some(tpe),
        Type::Sym(_, s) if t.u.symbol(s).kind == Kind::Type => {
            t.complete(s);
            let info = t.u.info(s);
            match *t.u.ty(info) {
                Type::Alias(a) => underlying_class_ref(t, a, depth),
                Type::Lambda { .. } => underlying_class_ref(t, info, depth),
                _ => None,
            }
        }
        Type::Lambda { result, .. } => underlying_class_ref(t, result, depth),
        Type::Applied(tycon, _) => underlying_class_ref(t, tycon, depth),
        _ => None,
    }
}
