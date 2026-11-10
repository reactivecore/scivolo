// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! Synthesized `Manifest[T]` and `OptManifest[T]` (dotty's `manifestFactoryOf`): the singleton
//! manifests of primitive and special classes, else calls of `ManifestFactory.arrayType`,
//! `wildcardType` and `classType`; `NoManifest` for an `OptManifest` of another type.

use super::Synthesizer;
use super::class_tag_erasure::{erased_class_type, has_stable_erasure};
use crate::defn::ARRAY;
use crate::tpd::TNodeId;
use crate::typer::{Pt, Typer};
use syntax::ast::{NodeId, TName, Tree};
use tasty::{Kind, Type, TypeId};

const MANIFEST_FACTORY: &str = "scala.reflect.ManifestFactory";

const PRIMITIVES: [&str; 9] = ["Byte", "Short", "Char", "Int", "Long", "Float", "Double", "Boolean", "Unit"];

/// The manifest asked for: a `Manifest`, an `OptManifest`, or the `ClassManifest` of an array
/// element of an `OptManifest`.
#[derive(Clone, Copy, PartialEq)]
pub enum ManifestKind {
    Full,
    Opt,
    Clss,
}

impl Synthesizer {
    /// Synthesizes the manifest of `kind` for `arg`, inferring the type arguments still open in
    /// `arg`; `None` for a `Manifest` of a type without a runtime class, e.g. a type parameter.
    pub fn manifest(
        self,
        t: &mut Typer<'_>,
        arg: TypeId,
        kind: ManifestKind,
    ) -> Result<Option<TNodeId>, &'static str> {
        let arg = t.fully_defined_as(arg, true)?;
        self.manifest_or_none(t, arg, kind, true)
    }

    /// The manifest of `tpe`, `NoManifest` in place of none for an `OptManifest`.
    fn manifest_or_none(
        self,
        t: &mut Typer<'_>,
        tpe: TypeId,
        kind: ManifestKind,
        top: bool,
    ) -> Result<Option<TNodeId>, &'static str> {
        match self.manifest_of(t, tpe, kind, top)? {
            None if kind == ManifestKind::Opt => {
                let module = t.resolve_full_name("scala.reflect.NoManifest", false).ok_or("NoManifest")?;
                let module_tpe = crate::defn::type_ref(t.u, module);
                Ok(Some(t.build().ident(module, module_tpe, self.at)))
            }
            found => Ok(found),
        }
    }

    fn manifest_of(
        self,
        t: &mut Typer<'_>,
        tpe: TypeId,
        kind: ManifestKind,
        top: bool,
    ) -> Result<Option<TNodeId>, &'static str> {
        let tpe = t.u.dealias(tpe);
        if let Some(&[elem]) = t.class_args(tpe, ARRAY).as_deref() {
            let elem_kind = if kind == ManifestKind::Full { kind } else { ManifestKind::Clss };
            let Some(e) = self.manifest_or_none(t, elem, elem_kind, top)? else { return Ok(None) };
            return self.factory_manifest(t, kind, "arrayType", elem, &[e]).map(Some);
        }
        if let Type::Bounds(_, hi) = *t.u.types().get(tpe) {
            if kind != ManifestKind::Full {
                return Ok(None);
            }
            let nothing = self.singleton_manifest(t, "Nothing")?;
            let Some(upper) = self.manifest_or_none(t, hi, ManifestKind::Full, top)? else {
                return Ok(None);
            };
            return self.factory_manifest(t, kind, "wildcardType", tpe, &[nothing, upper]).map(Some);
        }
        let class = t.u.class_symbol(tpe);
        let abstract_type = t.referenced(tpe).is_some_and(|s| {
            let s = t.u.symbol(s);
            s.kind == Kind::TypeParam || s.kind == Kind::Type && !s.flags.has(tasty::tags::OPAQUE)
        });
        let b = t.u.builtins();
        let bottom = class.is_some_and(|c| c == b.nothing || c == b.null);
        if abstract_type || !has_stable_erasure(t, tpe) || top && bottom {
            return Ok(None);
        }
        let name = class.map(|c| t.u.full_name(c)).unwrap_or_default();
        let special = match name.strip_prefix("scala.") {
            Some(n) if PRIMITIVES.contains(&n) || ["Any", "AnyVal", "Null", "Nothing"].contains(&n) => {
                Some(n)
            }
            _ if name == "java.lang.Object" => Some("Object"),
            _ => None,
        };
        if let Some(special) = special {
            return self.singleton_manifest(t, special).map(Some);
        }
        let erased = erased_class_type(t, tpe).ok_or("Manifest of a type erasing to no class")?;
        let class_class = t.resolve_full_name("java.lang.Class", true).ok_or("java.lang.Class")?;
        let literal_tpe = crate::defn::applied(t.u, class_class, vec![erased]);
        let literal =
            t.tpd.node(crate::tpd::Tree::ClassLiteral { tpe: erased }, self.at, Some(literal_tpe), None);
        let class_of_t = crate::defn::applied(t.u, class_class, vec![tpe]);
        let mut args = vec![t.type_cast(literal, class_of_t)];
        if kind == ManifestKind::Full
            && let Type::Applied(_, targs) = t.u.types().get(tpe).clone()
        {
            for a in targs {
                let Some(m) = self.manifest_or_none(t, a, ManifestKind::Full, false)? else {
                    return Ok(None);
                };
                args.push(m);
            }
        }
        self.factory_manifest(t, kind, "classType", tpe, &args).map(Some)
    }

    /// `ManifestFactory.name` of a primitive or special class.
    fn singleton_manifest(self, t: &mut Typer<'_>, name: &str) -> Result<TNodeId, &'static str> {
        let module = t.resolve_full_name(MANIFEST_FACTORY, false).ok_or("ManifestFactory")?;
        let member = t.resolve_full_name(&format!("scala.reflect.ManifestFactory$.{name}"), false);
        let member = member.ok_or("ManifestFactory member")?;
        let tpe = crate::defn::type_ref(t.u, module);
        let qualifier = t.build().ident(module, tpe, self.at);
        Ok(t.build().select(qualifier, member, self.at))
    }

    /// `factory.name[tpe](args)`, the factory `ManifestFactory` for a full manifest, else
    /// `ClassManifestFactory`, the alternative chosen by the arguments.
    fn factory_manifest(
        self,
        t: &mut Typer<'_>,
        kind: ManifestKind,
        name: &str,
        tpe: TypeId,
        args: &[TNodeId],
    ) -> Result<TNodeId, &'static str> {
        let factory = match kind {
            ManifestKind::Full => MANIFEST_FACTORY,
            _ => "scala.reflect.ClassManifestFactory",
        };
        let module = t.resolve_full_name(factory, false).ok_or("ManifestFactory")?;
        let module_tpe = crate::defn::type_ref(t.u, module);
        let module_ref = t.build().ident(module, module_tpe, self.at);
        let qualifier = t.spliced(module_ref);
        let name = TName { name: t.name(name), is_type: false };
        let mut fun = self.node(t, Tree::Select { qualifier, name });
        let tpe = match *t.u.types().get(tpe) {
            Type::Bounds(_, hi) => hi,
            _ => tpe,
        };
        let targ = t.type_tree(tpe, self.at, false);
        let targs = [t.spliced(targ)];
        let targs = t.ast.list(&targs);
        fun = self.node(t, Tree::TypeApply { fun, args: targs });
        let args: Vec<NodeId> = args.iter().map(|&a| t.spliced(a)).collect();
        let args = t.ast.list(&args);
        let apply = self.node(t, Tree::Apply { fun, args });
        let typed = t.typed(apply, Pt::Any, self.ctx);
        if t.is_error(typed) {
            return Err("Manifest factory call");
        }
        Ok(typed)
    }
}
