//! dotty's `SyntheticMembers` as PostTyper runs it on each template: `writeReplace` of static
//! objects, the members of case classes (`case_members.rs`).

use super::case_members::case_members;
use super::mirrors::mirror_support;
use crate::definitions::{applied, type_ref};
use crate::symbols::{is_module_class, is_static, source_module};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{PRIVATE, SYNTHETIC};
use tasty::{Flags, Kind, Symbol, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Adds the synthetic members of class definition `td`.
pub fn add_synthetic_members(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let span = l.tpd.span(td);
    let mut members = serializable_object_method(l, class, span)?;
    members.extend(case_members(l, class, span)?);
    let mirror = mirror_support(l, class, span)?;
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
/// static object `C` without one.
fn serializable_object_method(l: &mut Lower, class: SymbolId, span: Span) -> Result<Vec<TNodeId>> {
    if !is_module_class(l.u, class) || !is_static(l.u, class) || has_write_replace(l, class) {
        return Ok(Vec::new());
    }
    let span = if span.exists() { Span::synthetic(span.point(), span.point()) } else { span };
    let Some(module) = source_module(l.u, class) else {
        return unsupported(span, "an object without its value");
    };
    let any_ref = type_ref(l.u, l.defs.any_ref);
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method {
        binder,
        params: Vec::new(),
        result: any_ref,
        implicit: false,
        contextual: false,
    });
    let sym = l.u.define(Symbol {
        name: "writeReplace".into(),
        kind: Kind::Def,
        owner: Some(class),
        flags: Flags::default().with(PRIVATE).with(SYNTHETIC),
        target: None,
        is_extern: false,
        decls: Vec::new(),
        tree: None,
    });
    l.u.set_info(sym, info);

    let proxy = l.defs.module_serialization_proxy;
    let new = l.new_(proxy, span);
    let ctor = l.select(new, l.defs.module_serialization_proxy_ctor, span);
    let module_ref = type_ref(l.u, module);
    let class_of = applied(l.u, l.defs.class, vec![module_ref]);
    let lit = l.tpd.node(Tree::ClassLiteral { tpe: module_ref }, span, Some(class_of), None);
    let proxy_type = l.tpd.tpe(new);
    let rhs = l.apply(ctor, &[lit], proxy_type, span);

    let name = l.term_name("writeReplace");
    let no_params = l.tpd.list(&[]);
    let paramss = l.tpd.clauses(&[no_params]);
    let tpt = l.type_tree(any_ref, span);
    let def = l.tpd.node(Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) }, span, None, Some(sym));
    Ok(vec![def])
}

/// Whether `class` has a member `writeReplace(): AnyRef`.
fn has_write_replace(l: &mut Lower, class: SymbolId) -> bool {
    let object = tasty::Erased::Class("java.lang.Object".into());
    l.u.member(class, "writeReplace")
        .into_iter()
        .any(|m| l.u.symbol(m).kind == Kind::Def && l.u.erased_method(m) == (Vec::new(), object.clone()))
}
