// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//! dotty's `SyntheticMembers` as PostTyper runs it on each template: `writeReplace` of static
//! objects, the members of enum values (`enum_values.rs`), case classes (`case_members.rs`) and
//! value classes (`value_members.rs`).

use super::case_members::case_members;
use super::enum_values::enum_value_members;
use super::mirrors::mirror_support;
use super::value_members::value_members;
use super::walk::class_defs_inner_first;
use crate::definitions::{applied, symbol};
use crate::{Lower, Result, unsupported};
use names::nme;
use syntax::ast::Span;
use tasty::tags::{PRIVATE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Adds the synthetic members each class of unit `root` lacks, inner classes first.
pub fn add_all(l: &mut Lower, root: TNodeId) -> Result<()> {
    class_defs_inner_first(l.tpd, root).into_iter().try_for_each(|td| add_synthetic_members(l, root, td))
}

/// Adds the synthetic members of class definition `td` of unit `root`.
fn add_synthetic_members(l: &mut Lower, root: TNodeId, td: TNodeId) -> Result<()> {
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
    let mirror = match l.u.is_scala2(class) {
        true => None,
        false => mirror_support(l, root, class, span)?,
    };
    if members.is_empty() && mirror.is_none() {
        return Ok(());
    }
    members.extend_from_slice(l.tpd.items(body));
    let mut parents = l.tpd.items(parents).to_vec();
    if let Some((parent, defs)) = mirror {
        members.extend(defs);
        let parent = pickled_ref(l, parent);
        let mut types = l.u.parents(class);
        types.push(parent);
        l.u.set_parents(class, types);
        let focus = l.tpd.span(template).start_pos().to_synthetic();
        parents.push(l.type_tree(parent, focus));
    }
    let body = l.tpd.list(&members);
    let parents = l.tpd.list(&parents);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// `private def writeReplace(): AnyRef = new ModuleSerializationProxy(classOf[C.type])` for a
/// static object `C` without one (dotty's `serializableObjectMethod`).
fn serializable_object_method(l: &mut Lower, class: SymbolId, span: Span) -> Result<Vec<TNodeId>> {
    if !l.u.is_static_module(class) || has_serialization_method(l, class, nme::WRITE_REPLACE) {
        return Ok(Vec::new());
    }
    let span = if span.exists() { Span::synthetic(span.point(), span.point()) } else { span };
    let Some(module) = l.u.source_module(class) else {
        return unsupported(span, "an object without its value");
    };
    let (sym, any_ref) = private_method(l, class, nme::WRITE_REPLACE);
    let s = l.serialization()?;
    let proxy = pickled_ref(l, s.proxy);
    let proxy_tpt = l.type_tree(proxy, span);
    let new = l.tpd.node(Tree::New { tpt: proxy_tpt }, span, Some(proxy), None);
    let ctor = l.select(new, s.proxy_ctor, span);
    let module_ref = pickled_ref(l, module);
    let class_of = applied(l.u, s.class, vec![module_ref]);
    let lit = l.tpd.node(Tree::ClassLiteral { tpe: module_ref }, span, Some(class_of), None);
    let proxy_type = l.tpd.tpe(new);
    let rhs = l.apply(ctor, &[lit], proxy_type, span);
    let tpt = l.type_tree(any_ref, span);
    Ok(vec![l.build().def_def(sym, &[&[]], tpt, Some(rhs), span)])
}

/// A new `private def name(): AnyRef` of `class`, and `AnyRef`.
pub fn private_method(l: &mut Lower, class: SymbolId, name: &str) -> (SymbolId, TypeId) {
    let any_ref = pickled_ref(l, l.u.defn().any_ref);
    let binder = l.u.fresh_binder();
    let info = l.u.intern_method(binder, &[], any_ref, false, false);
    let sym = l.u.define(symbol(name, Kind::Def, class, Flags::default().with(PRIVATE).with(SYNTHETIC)));
    l.u.set_info(sym, info);
    (sym, any_ref)
}

/// The type of a reference to `s` as the typer gives it: a member of a class through the class's
/// `this`, else without prefix (dotty's `typeRef`).
pub fn pickled_ref(l: &mut Lower, s: SymbolId) -> TypeId {
    let prefix = match l.u.symbol(s).owner {
        Some(o) if l.u.symbol(o).kind == Kind::Class => {
            let class = pickled_ref(l, o);
            Some(l.u.intern_type(Type::This(class)))
        }
        _ => None,
    };
    l.u.intern_type(Type::Sym(prefix, s))
}

/// The member synthetic member `m` of a class compiled in the same run overrides, the first
/// concrete one in linearization order, else the first: what dotty's typer selects before
/// `SyntheticMembers` adds `m`.
pub fn before_synthesized(l: &mut Lower, m: SymbolId) -> Option<SymbolId> {
    const SYNTHESIZED: [&str; 9] = [
        nme::HASH_CODE,
        nme::EQUALS,
        nme::TO_STRING,
        nme::CAN_EQUAL,
        nme::PRODUCT_ARITY,
        nme::PRODUCT_PREFIX,
        nme::PRODUCT_ELEMENT,
        nme::PRODUCT_ELEMENT_NAME,
        nme::ORDINAL,
    ];
    let s = l.u.symbol(m);
    let owner = s.owner.filter(|_| s.flags.has(SYNTHETIC) && SYNTHESIZED.contains(&s.name.as_str()))?;
    if !l.u.compiles_scala_library() || !l.u.is_same_run(owner) {
        return None;
    }
    let name = s.name.clone();
    let site = l.u.intern_type(Type::Sym(None, owner));
    let site = l.u.intern_type(Type::This(site));
    let mut deferred = None;
    for base in l.u.linearization(owner).into_iter().skip(1) {
        for other in l.u.decls_named(base, &name) {
            if l.u.symbol(other).kind == Kind::Def && l.u.info_matches(site, m, other) {
                if !crate::codegen::forwarded_members::is_deferred(l, other) {
                    return Some(other);
                }
                deferred = deferred.or(Some(other));
            }
        }
    }
    deferred
}

/// Whether `class` has a member `name(): AnyRef`, `writeReplace` or `readResolve`.
pub fn has_serialization_method(l: &mut Lower, class: SymbolId, name: &str) -> bool {
    let object = tasty::Erased::object();
    l.u.member(class, name)
        .into_iter()
        .any(|m| l.u.symbol(m).kind == Kind::Def && l.u.erased_method(m) == (Vec::new(), object.clone()))
}
