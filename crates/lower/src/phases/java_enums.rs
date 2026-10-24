// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/CompleteJavaEnums.scala
//! Enums extending `java.lang.Enum` (dotty's `CompleteJavaEnums` without the static forwarders):
//! the enum class's constructor takes `_$name` and `_$ordinal` and passes them to `java.lang.Enum`'s;
//! the class of each value passes its name and ordinal.

use super::enum_values::{child_index, param};
use crate::definitions::symbol;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{ENUM, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Whether class `c` derives from `java.lang.Enum`.
pub fn derives_from_java_enum(l: &mut Lower, c: SymbolId) -> bool {
    l.u.linearization(c).into_iter().any(|b| l.u.full_name(b) == "java.lang.Enum")
}

/// Completes the enum classes of `classes` of unit `root` that extend `java.lang.Enum`, then the
/// classes of their values.
pub fn transform(l: &mut Lower, root: TNodeId, classes: &[TNodeId]) -> Result<()> {
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        if has(l.u, class, ENUM) && !l.u.is_module_class(class) && derives_from_java_enum(l, class) {
            complete_enum_class(l, td)?;
        }
    }
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        if super::enum_values::is_enum_value(l, class) && derives_from_java_enum(l, class) {
            complete_value_class(l, root, td, class)?;
        }
    }
    Ok(())
}

/// Adds `_$name: String, _$ordinal: Int` to the constructor of the enum class `td` defines and
/// passes them to the constructor of `java.lang.Enum`.
fn complete_enum_class(l: &mut Lower, td: TNodeId) -> Result<()> {
    let (constr, parents) = template_parts(l, td);
    let ctor = l.tpd.sym(constr).expect("a constructor's symbol");
    let span = l.tpd.span(constr);
    let Tree::DefDef { name, paramss, tpt, rhs } = *l.tpd.tree(constr) else { unreachable!("a constructor") };
    let mut clauses = l.tpd.clause_items(paramss).to_vec();
    let Some(last) = clauses.last_mut() else {
        return unsupported(span, "an enum constructor without parameters");
    };
    let flags = Flags::default().with(SYNTHETIC);
    let string = l.u.class_ref("java.lang.String");
    let int = l.u.class_ref("scala.Int");
    let mut added = Vec::new();
    for (n, t) in [("_$name", string), ("_$ordinal", int)] {
        let p = l.u.define(symbol(n, Kind::Param, ctor, flags));
        l.u.set_info(p, t);
        added.push(p);
    }
    let defs: Vec<TNodeId> = added.iter().map(|&p| l.param_def(p, span)).collect();
    let params = [l.tpd.items(*last), &defs].concat();
    *last = l.tpd.list(&params);
    let paramss = l.tpd.clauses(&clauses);
    l.tpd.get_mut(constr).tree = Tree::DefDef { name, paramss, tpt, rhs };
    let info = l.u.info(ctor);
    let info = with_name_and_ordinal(l, info);
    l.u.set_info(ctor, info);
    let args: Vec<TNodeId> = added.iter().map(|&p| l.ident(p, Span::NONE)).collect();
    let enum_parent = parents.into_iter().find(|&p| {
        let called = called_class(l, p);
        called.is_some_and(|c| l.u.full_name(c) == "java.lang.Enum")
    });
    let Some(parent) = enum_parent else {
        return unsupported(l.tpd.span(td), "an enum extending java.lang.Enum through another class");
    };
    add_args(l, parent, &args, None);
    Ok(())
}

/// Passes the name and ordinal of the enum value whose class `class` defines `td` in unit
/// `root` to its enum class's constructor: the parameters of `$new`, or literals for a case.
fn complete_value_class(l: &mut Lower, root: TNodeId, td: TNodeId, class: SymbolId) -> Result<()> {
    let (_, parents) = template_parts(l, td);
    let owner = l.u.symbol(class).owner.expect("a local class's owner");
    let span = l.tpd.span(td);
    let args = if l.u.symbol(owner).name == "$new" {
        let (Some(name), Some(ordinal)) =
            (param(l, root, owner, "$name"), param(l, root, owner, "_$ordinal"))
        else {
            return unsupported(span, "an enum value constructor without its parameters");
        };
        vec![l.ident(name, Span::NONE), l.ident(ordinal, Span::NONE)]
    } else {
        let Some(ordinal) = child_index(l, owner) else {
            return unsupported(span, "an enum value outside its enum's cases");
        };
        let name = l.u.symbol(owner).name.clone();
        vec![l.string_literal(&name), l.int_literal(ordinal)]
    };
    let Some(&parent) = parents.first() else { return unsupported(span, "an enum value without its enum") };
    let Some(enum_class) = called_class(l, parent) else {
        return unsupported(span, "an enum value without a call of its enum's constructor");
    };
    let ctor = l.u.decl(enum_class, "<init>").unwrap_or_default().into_iter().next();
    let info = ctor.map(|c| l.u.info(c));
    add_args(l, parent, &args, info);
    Ok(())
}

fn template_parts(l: &Lower, td: TNodeId) -> (TNodeId, Vec<TNodeId>) {
    let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, .. } = *l.tpd.tree(rhs) else { unreachable!("a template") };
    (constr, l.tpd.items(parents).to_vec())
}

/// The class whose constructor parent call `p` calls.
fn called_class(l: &Lower, p: TNodeId) -> Option<SymbolId> {
    let mut fun = p;
    loop {
        match *l.tpd.tree(fun) {
            Tree::Apply { fun: f, .. } | Tree::TypeApply { fun: f, .. } => fun = f,
            Tree::Block { expr, .. } => fun = expr,
            _ => break,
        }
    }
    let ctor = l.tpd.sym(fun).filter(|&s| l.u.is_constructor(s))?;
    l.u.symbol(ctor).owner
}

/// `args` appended to the last argument list of constructor call `call`, whose method type
/// becomes `info` with the parameters of its last clause completed if given, else `info` of the
/// call's own with them added.
fn add_args(l: &mut Lower, call: TNodeId, args: &[TNodeId], info: Option<TypeId>) {
    let call = match *l.tpd.tree(call) {
        Tree::Block { expr, .. } => expr,
        _ => call,
    };
    let Tree::Apply { fun, args: old } = *l.tpd.tree(call) else { return };
    let joined = [l.tpd.items(old), args].concat();
    let list = l.tpd.list(&joined);
    l.tpd.get_mut(call).tree = Tree::Apply { fun, args: list };
    let fun_tpe = l.tpd.get(fun).tpe;
    let tpe = match (info, fun_tpe) {
        (Some(i), _) => Some(last_clause(l, i)),
        (None, Some(t)) => Some(with_name_and_ordinal(l, t)),
        _ => None,
    };
    l.tpd.get_mut(fun).tpe = tpe;
}

/// The method type of the last clause of method type `t`.
fn last_clause(l: &Lower, t: TypeId) -> TypeId {
    match *l.u.types().get(t) {
        Type::Poly { result, .. } => last_clause(l, result),
        Type::Method { result, .. } if matches!(l.u.types().get(result), Type::Method { .. }) => {
            last_clause(l, result)
        }
        _ => t,
    }
}

/// Method or polymorphic type `t` with `_$name: String, _$ordinal: Int` added to its last
/// clause (dotty's `addConstrParams`).
fn with_name_and_ordinal(l: &mut Lower, t: TypeId) -> TypeId {
    match l.u.types().get(t).clone() {
        Type::Poly { binder, params, result } => {
            let result = with_name_and_ordinal(l, result);
            l.u.intern_type(Type::Poly { binder, params, result })
        }
        Type::Method { binder, params, result, implicit, contextual } => {
            if matches!(l.u.types().get(result), Type::Method { .. }) {
                let result = with_name_and_ordinal(l, result);
                return l.u.intern_type(Type::Method { binder, params, result, implicit, contextual });
            }
            let string = l.u.class_ref("java.lang.String");
            let int = l.u.class_ref("scala.Int");
            let mut params = params;
            params.extend([("_$name".to_owned(), string), ("_$ordinal".to_owned(), int)]);
            l.u.intern_type(Type::Method { binder, params, result, implicit, contextual })
        }
        _ => t,
    }
}
