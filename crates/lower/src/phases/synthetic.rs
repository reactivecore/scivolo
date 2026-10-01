//! dotty's `SyntheticMembers` as PostTyper runs it on each template: `writeReplace` of static
//! objects.

use crate::defn::type_ref;
use crate::symbols::{is_module_class, is_static, source_module};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{PRIVATE, SYNTHETIC};
use tasty::{Flags, Kind, Symbol, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

pub fn add_synthetic_members(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let mut members = serializable_object_method(l, class, l.tpd.span(td))?;
    if members.is_empty() {
        return Ok(());
    }
    members.extend_from_slice(l.tpd.items(body));
    let body = l.tpd.list(&members);
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
    let any_ref = type_ref(l.u, l.defn.any_ref);
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method { binder, params: Vec::new(), result: any_ref, implicit: false });
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

    let proxy = l.defn.module_serialization_proxy;
    let new = l.new_(proxy, span);
    let ctor = l.select(new, l.defn.module_serialization_proxy_ctor, span);
    let module_ref = type_ref(l.u, module);
    let class_of = crate::defn::applied(l.u, l.defn.class, vec![module_ref]);
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
