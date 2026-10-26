// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! Synthesized `Manifest[T]` and `OptManifest[T]` (dotty's `manifestFactoryOf`): the singleton
//! manifests of primitive and special classes, else calls of `ManifestFactory.arrayType`,
//! `wildcardType` and `classType`; `NoManifest` for an `OptManifest` of another type.

use super::super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, Type, TypeId};

const PRIMITIVES: [&str; 9] = ["Byte", "Short", "Char", "Int", "Long", "Float", "Double", "Boolean", "Unit"];

#[derive(Clone, Copy, PartialEq)]
pub enum ManifestKind {
    Full,
    Opt,
    Clss,
}

impl Typer<'_> {
    pub fn synthesized_manifest(
        &mut self,
        arg: TypeId,
        kind: ManifestKind,
        at: Span,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, &'static str> {
        let arg = self.fully_defined_as(arg, true)?;
        self.manifest_or_none(arg, kind, true, (at, ctx))
    }

    /// The manifest of `t`, `NoManifest` in place of none for an `OptManifest`.
    fn manifest_or_none(
        &mut self,
        t: TypeId,
        kind: ManifestKind,
        top: bool,
        (at, ctx): (Span, Ctx),
    ) -> Result<Option<TNodeId>, &'static str> {
        match self.manifest_of(t, kind, top, (at, ctx))? {
            None if kind == ManifestKind::Opt => {
                let module = self.resolve_full_name("scala.reflect.NoManifest", false).ok_or("NoManifest")?;
                let tpe = crate::defn::type_ref(self.u, module);
                Ok(Some(self.build().ident(module, tpe, at)))
            }
            found => Ok(found),
        }
    }

    fn manifest_of(
        &mut self,
        t: TypeId,
        kind: ManifestKind,
        top: bool,
        (at, ctx): (Span, Ctx),
    ) -> Result<Option<TNodeId>, &'static str> {
        let t = self.u.dealias(t);
        if let Some(&[elem]) = self.class_args(t, "scala.Array").as_deref() {
            let elem_kind = if kind == ManifestKind::Full { kind } else { ManifestKind::Clss };
            let Some(e) = self.manifest_or_none(elem, elem_kind, top, (at, ctx))? else { return Ok(None) };
            return self.factory_manifest(kind, "arrayType", elem, &[e], (at, ctx)).map(Some);
        }
        if let Type::Bounds(_, hi) = *self.u.types().get(t) {
            if kind != ManifestKind::Full {
                return Ok(None);
            }
            let nothing = self.singleton_manifest("Nothing", at)?;
            let Some(upper) = self.manifest_or_none(hi, ManifestKind::Full, top, (at, ctx))? else {
                return Ok(None);
            };
            return self.factory_manifest(kind, "wildcardType", t, &[nothing, upper], (at, ctx)).map(Some);
        }
        let class = self.u.class_symbol(t);
        let abstract_type = self.referenced(t).is_some_and(|s| {
            let s = self.u.symbol(s);
            s.kind == Kind::TypeParam || s.kind == Kind::Type && !s.flags.has(tasty::tags::OPAQUE)
        });
        let b = self.u.builtins();
        let bottom = class.is_some_and(|c| c == b.nothing || c == b.null);
        if abstract_type || !self.has_stable_erasure(t) || top && bottom {
            return Ok(None);
        }
        let name = class.map(|c| self.u.full_name(c)).unwrap_or_default();
        let special = match name.strip_prefix("scala.") {
            Some(n) if PRIMITIVES.contains(&n) || ["Any", "AnyVal", "Null", "Nothing"].contains(&n) => {
                Some(n)
            }
            _ if name == "java.lang.Object" => Some("Object"),
            _ => None,
        };
        if let Some(special) = special {
            return self.singleton_manifest(special, at).map(Some);
        }
        let erased = self.erased_class_type(t).ok_or("Manifest of a type erasing to no class")?;
        let class_class = self.resolve_full_name("java.lang.Class", true).ok_or("java.lang.Class")?;
        let literal_tpe = crate::defn::applied(self.u, class_class, vec![erased]);
        let literal =
            self.tpd.node(crate::tpd::Tree::ClassLiteral { tpe: erased }, at, Some(literal_tpe), None);
        let class_of_t = crate::defn::applied(self.u, class_class, vec![t]);
        let mut args = vec![self.type_cast(literal, class_of_t)];
        if kind == ManifestKind::Full
            && let Type::Applied(_, targs) = self.u.types().get(t).clone()
        {
            for a in targs {
                let Some(m) = self.manifest_or_none(a, ManifestKind::Full, false, (at, ctx))? else {
                    return Ok(None);
                };
                args.push(m);
            }
        }
        self.factory_manifest(kind, "classType", t, &args, (at, ctx)).map(Some)
    }

    /// `ManifestFactory.name` of a primitive or special class.
    fn singleton_manifest(&mut self, name: &str, at: Span) -> Result<TNodeId, &'static str> {
        let module =
            self.resolve_full_name("scala.reflect.ManifestFactory", false).ok_or("ManifestFactory")?;
        let member = self.resolve_full_name(&format!("scala.reflect.ManifestFactory$.{name}"), false);
        let member = member.ok_or("ManifestFactory member")?;
        let tpe = crate::defn::type_ref(self.u, module);
        let qualifier = self.build().ident(module, tpe, at);
        Ok(self.build().select(qualifier, member, at))
    }

    /// `factory.name[t](args)`, the factory `ManifestFactory` for a full manifest, else
    /// `ClassManifestFactory`, the alternative chosen by the arguments.
    fn factory_manifest(
        &mut self,
        kind: ManifestKind,
        name: &str,
        t: TypeId,
        args: &[TNodeId],
        (at, ctx): (Span, Ctx),
    ) -> Result<TNodeId, &'static str> {
        let factory = match kind {
            ManifestKind::Full => "scala.reflect.ManifestFactory",
            _ => "scala.reflect.ClassManifestFactory",
        };
        let module = self.resolve_full_name(factory, false).ok_or("ManifestFactory")?;
        let tpe = crate::defn::type_ref(self.u, module);
        let module_ref = self.build().ident(module, tpe, at);
        let qualifier = self.spliced(module_ref);
        let name = TName { name: self.name(name), is_type: false };
        let mut fun = self.at(Tree::Select { qualifier, name }, at);
        let t = match *self.u.types().get(t) {
            Type::Bounds(_, hi) => hi,
            _ => t,
        };
        let targ = self.type_tree(t, at, false);
        let targs = [self.spliced(targ)];
        let targs = self.ast.list(&targs);
        fun = self.at(Tree::TypeApply { fun, args: targs }, at);
        let args: Vec<NodeId> = args.iter().map(|&a| self.spliced(a)).collect();
        let args = self.ast.list(&args);
        let apply = self.at(Tree::Apply { fun, args }, at);
        let typed = self.typed(apply, Pt::Any, ctx);
        if self.is_error(typed) {
            return Err("Manifest factory call");
        }
        Ok(typed)
    }

    fn at(&mut self, tree: Tree, span: Span) -> NodeId {
        let node = self.ast.node(tree);
        self.ast.with_span(node, span);
        node
    }
}
