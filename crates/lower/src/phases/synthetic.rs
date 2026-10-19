// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//! dotty's `SyntheticMembers` as PostTyper runs it on each template: `writeReplace` of static
//! objects, the members of enum values (`enum_values.rs`), case classes (`case_members.rs`) and
//! value classes (`value_members.rs`).

use super::case_members::case_members;
use super::enum_values::enum_value_members;
use super::mirrors::mirror_support;
use super::value_members::value_members;
use crate::definitions::{applied, symbol, type_ref};
use crate::symbols::is_static_module;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{PRIVATE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Adds the synthetic members of class definition `td` of unit `root`.
pub fn add_synthetic_members(l: &mut Lower, root: TNodeId, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let span = l.tpd.span(td);
    let mut members = serializable_object_method(l, class, span)?;
    members.extend(enum_value_members(l, root, class, span)?);
    members.extend(case_members(l, class, span)?);
    members.extend(value_members(l, class, span)?);
    let mirror = mirror_support(l, root, class, span)?;
    if members.is_empty() && mirror.is_none() {
        return Ok(());
    }
    members.extend_from_slice(l.tpd.items(body));
    let mut parents = l.tpd.items(parents).to_vec();
    if let Some((parent, defs)) = mirror {
        members.extend(defs);
        let parent = type_ref(l.u, parent);
        let mut types = l.u.parents(class);
        types.push(parent);
        l.u.set_parents(class, types);
        parents.push(l.type_tree(parent, Span::NONE));
    }
    let body = l.tpd.list(&members);
    let parents = l.tpd.list(&parents);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// `private def writeReplace(): AnyRef = new ModuleSerializationProxy(classOf[C.type])` for a
/// static object `C` without one (dotty's `serializableObjectMethod`).
fn serializable_object_method(l: &mut Lower, class: SymbolId, span: Span) -> Result<Vec<TNodeId>> {
    if !is_static_module(l.u, class) || has_write_replace(l, class) {
        return Ok(Vec::new());
    }
    let span = if span.exists() { Span::synthetic(span.point(), span.point()) } else { span };
    let Some(module) = l.u.source_module(class) else {
        return unsupported(span, "an object without its value");
    };
    let (sym, any_ref) = private_method(l, class, "writeReplace");
    let s = l.serialization()?;
    let new = l.new_(s.proxy, span);
    let ctor = l.select(new, s.proxy_ctor, span);
    let module_ref = type_ref(l.u, module);
    let class_of = applied(l.u, s.class, vec![module_ref]);
    let lit = l.tpd.node(Tree::ClassLiteral { tpe: module_ref }, span, Some(class_of), None);
    let proxy_type = l.tpd.tpe(new);
    let rhs = l.apply(ctor, &[lit], proxy_type, span);
    let tpt = l.type_tree(any_ref, span);
    Ok(vec![l.build().def_def(sym, &[&[]], tpt, Some(rhs), span)])
}

/// A new `private def name(): AnyRef` of `class`, and `AnyRef`.
pub fn private_method(l: &mut Lower, class: SymbolId, name: &str) -> (SymbolId, TypeId) {
    let any_ref = type_ref(l.u, l.defs.any_ref);
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method {
        binder,
        params: Vec::new(),
        result: any_ref,
        implicit: false,
        contextual: false,
    });
    let sym = l.u.define(symbol(name, Kind::Def, class, Flags::default().with(PRIVATE).with(SYNTHETIC)));
    l.u.set_info(sym, info);
    (sym, any_ref)
}

/// Whether `class` has a member `writeReplace(): AnyRef`.
fn has_write_replace(l: &mut Lower, class: SymbolId) -> bool {
    let object = tasty::Erased::Class("java.lang.Object".into());
    l.u.member(class, "writeReplace")
        .into_iter()
        .any(|m| l.u.symbol(m).kind == Kind::Def && l.u.erased_method(m) == (Vec::new(), object.clone()))
}
