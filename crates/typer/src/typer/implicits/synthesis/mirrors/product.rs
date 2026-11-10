// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! The product mirrors of case classes and singletons (dotty's `productMirror`): the companion,
//! the singleton or an anonymous class, cast to the mirror type.

use super::formal::MirrorCore;
use super::sources::MirrorSource;
use super::{MirrorSynthesis, label_pairs};
use crate::tpd::TNodeId;
use crate::typer::Typer;
use crate::typer::small_tuples::tuple_class_args;
use tasty::tags::{CASE, CASEaccessor, PRIVATE, PROTECTED};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

impl MirrorSynthesis {
    /// Synthesizes the product mirror of a tuple, a case object, an enum value or a case class;
    /// `None` for another class.
    pub fn product(self, t: &mut Typer<'_>, source: MirrorSource) -> Result<Option<TNodeId>, &'static str> {
        let (at, mirrored) = (self.at, self.formal.mirrored);
        let (pre, cls) = match source {
            MirrorSource::Tuple(elems) => return self.tuple(t, &elems, None),
            MirrorSource::NamedTuple(names, elems) => return self.tuple(t, &elems, Some(&names)),
            MirrorSource::Singleton(s, path) => return self.singleton(t, s, path).map(Some),
            MirrorSource::Class(pre, cls) => (pre, cls),
        };
        if let Some(args) = tuple_class_args(t.u, mirrored).filter(|a| !a.is_empty()) {
            return self.tuple(t, &args, None);
        }
        if !self.is_generic_product(t, cls) {
            return Ok(None);
        }
        let accessors = case_accessors(t, cls);
        let labels: Vec<String> = accessors.iter().map(|&a| t.u.symbol(a).name.clone()).collect();
        let site = match *t.u.types().get(mirrored) {
            Type::Lambda { result, .. } => result,
            _ => mirrored,
        };
        let elems: Vec<TypeId> = accessors
            .iter()
            .map(|&a| {
                t.complete(a);
                let info = t.u.info(a);
                let tpe = t.u.as_seen_from(info, site, cls);
                match *t.u.types().get(tpe) {
                    Type::ByName(r) => r,
                    _ => tpe,
                }
            })
            .collect();
        let nested = t.nested_pairs(&elems)?;
        let (mono, elem_types) = self.formal.mono_and_elems(t.u, nested);
        let elem_labels = label_pairs(t, &labels)?;
        let label = t.u.symbol(cls).name.clone();
        let core = MirrorCore {
            parent: "scala.deriving.Mirror$.Product",
            mono,
            label: &label,
            elems: Some((elem_types, elem_labels)),
        };
        let cast = self.mirror_type(t, core)?;
        let mirror = match product_companion(t.u, cls) {
            Some(module) => {
                let path = t.u.intern_type(Type::Sym(pre, module));
                let tree = t.path_tree(path, at, 8).ok_or("Mirror companion outside the subset")?;
                let class =
                    t.u.module_class(module)
                        .filter(|&c| crate::sealed::is_contained_in(t.u, self.ctx.owner, c));
                match class {
                    Some(class) if matches!(t.tpd.tree(tree), crate::tpd::Tree::Select { .. }) => {
                        let class_type = crate::defn::type_ref(t.u, class);
                        t.build().this(class_type, at)
                    }
                    _ => tree,
                }
            }
            None => self.anonymous(t, mono, false)?,
        };
        self.cast(t, mirror, cast)
    }

    /// Synthesizes the case object or enum value `s` at `path` cast to its `Mirror.Singleton`
    /// type; a Scala 2 object wrapped in `new Mirror.SingletonProxy(s)`.
    fn singleton(self, t: &mut Typer<'_>, s: SymbolId, path: TypeId) -> Result<TNodeId, &'static str> {
        let empty = t.nested_pairs(&[])?;
        let scala2 = t.u.is_module(s) && t.u.module_class(s).is_some_and(|c| t.u.is_scala2(c));
        let parent = match scala2 {
            true => "scala.deriving.Mirror$.SingletonProxy",
            false => "scala.deriving.Mirror$.Singleton",
        };
        let label = t.u.symbol(s).name.clone();
        self.formal.check_elems(&mut t.types(), empty, empty)?;
        let core = MirrorCore { parent, mono: self.formal.mirrored, label: &label, elems: None };
        let cast = self.mirror_type(t, core)?;
        let mut value = t.path_tree(path, self.at, 8).ok_or("Mirror of a singleton outside the subset")?;
        if scala2 {
            value = self.new_with_args(t, parent, &[value])?;
        }
        t.build().cast_to(value, cast, self.at)
    }

    /// Whether case class `c` gets a product mirror where this one is summoned (dotty's
    /// `isGenericProduct`): its companion serves as the mirror, or code here may call its
    /// constructor.
    pub fn is_generic_product(self, t: &mut Typer<'_>, c: SymbolId) -> bool {
        t.complete(c);
        if let Some(&ctor) = t.u.decl(c, "<init>").unwrap_or_default().first() {
            t.complete(ctor);
        }
        let value_class = t.is_derived_value_class(c);
        crate::sealed::is_product_shape(t.u, c, value_class)
            && (product_companion(t.u, c).is_some() || self.can_access_constructor(t, c))
    }

    /// Whether the primary constructor of `c` is accessible here for an anonymous mirror (dotty's
    /// `canAccessCtor`).
    fn can_access_constructor(self, t: &mut Typer<'_>, c: SymbolId) -> bool {
        let Some(&ctor) = t.u.decl(c, "<init>").unwrap_or_default().first() else { return true };
        let flags = t.u.symbol(ctor).flags;
        let owners: Vec<SymbolId> =
            std::iter::successors(Some(self.ctx.owner), |&o| t.u.symbol(o).owner).collect();
        let sub = owners.iter().any(|&o| t.u.is_class(o) && t.u.derives_from(o, c));
        let within = t.syms.private_within.get(&ctor).is_none_or(|w| owners.contains(w));
        (!(flags.has(PRIVATE) || flags.has(PROTECTED)) || sub) && within
    }
}

/// Lists the fields of the first parameter list of case class `c`, in order (dotty's
/// `caseAccessors`).
fn case_accessors(t: &mut Typer<'_>, c: SymbolId) -> Vec<SymbolId> {
    t.complete(c);
    let decls = t.u.decls(c).unwrap_or_default();
    let u = &*t.u;
    let accessor = |d: SymbolId| {
        matches!(u.symbol(d).kind, Kind::Val | Kind::Param) && u.symbol(d).flags.has(CASEaccessor)
    };
    decls.into_iter().filter(|&d| accessor(d)).collect()
}

/// The companion of class `c` if it is its mirror (dotty's `useCompanionAsProductMirror`).
fn product_companion(u: &mut Universe, c: SymbolId) -> Option<SymbolId> {
    if u.is_scala2(c) {
        return None;
    }
    let module = u.companion_module(c)?;
    let case = u.module_class(module).is_some_and(|m| u.symbol(m).flags.has(CASE));
    (!case).then_some(module)
}
