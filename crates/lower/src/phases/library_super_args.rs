// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/HoistSuperArgs.scala
//! The methods dotty's `HoistSuperArgs` gives a library class compiled in the same run for the
//! lambdas its member classes pass to their superclass constructor, which TASTy lacks.

use crate::Lower;
use crate::definitions::symbol;
use tasty::tags::{
    APPLY, BLOCK, DEFDEF, LAMBDA, PARAM, SPLITCLAUSE, SYNTHETIC, TEMPLATE, TYPEAPPLY, TYPEPARAM,
};
use tasty::{Ast, Flags, Kind, NodeId, SymbolId, Type};

/// `C$superArg$N(params)` in `class` for the `N`th argument defining a method member class `C`
/// passes to its superclass constructor, `params` those of the constructor of `C`, its result
/// the function class of that argument, a lambda; none for `C` if one is not a lambda. Each comes
/// with that constructor, whose span it has.
pub fn hoisted(l: &mut Lower, class: SymbolId) -> Vec<(SymbolId, SymbolId)> {
    let mut out = Vec::new();
    for c in l.u.decls(class).unwrap_or_default() {
        if l.u.symbol(c).kind != Kind::Class {
            continue;
        }
        let Some((file, typedef)) = l.u.symbol(c).tree else { continue };
        let arities = {
            let (ast, _) = l.u.file(file);
            let mut arities = Vec::new();
            if let Some(parent) = first_parent(ast, typedef) {
                lambda_args(ast, parent, &mut arities);
            }
            arities
        };
        let Some(arities) = arities.into_iter().collect::<Option<Vec<usize>>>() else { continue };
        let Some(&ctor) = l.u.decl(c, "<init>").unwrap_or_default().first() else { continue };
        let ctor_info = l.u.info(ctor);
        let params = l.u.method(ctor_info).map(|m| m.params.to_vec()).unwrap_or_default();
        let name = l.u.symbol(c).name.clone();
        for (i, arity) in arities.into_iter().enumerate() {
            let flags = Flags::default().with(SYNTHETIC);
            let m = l.u.define(symbol(&format!("{name}$superArg${}", i + 1), Kind::Def, class, flags));
            let result = l.u.class_ref(&format!("scala.Function{arity}"));
            let binder = l.u.fresh_binder();
            let method =
                Type::Method { binder, params: params.clone(), result, implicit: false, contextual: false };
            let info = l.u.intern_type(method);
            l.u.set_info(m, info);
            super::accessor_names::ensure_not_private(l, m);
            out.push((m, ctor));
        }
    }
    out
}

/// The superclass constructor call of class definition `typedef`.
fn first_parent(ast: &Ast, typedef: NodeId) -> Option<NodeId> {
    let &template = ast.kids(typedef).first()?;
    if ast.tag(template) != TEMPLATE {
        return None;
    }
    let kids = ast.kids(template);
    kids.iter()
        .take_while(|&&k| !matches!(ast.tag(k), DEFDEF | SPLITCLAUSE))
        .map(|&k| ast.deref(k))
        .find(|&k| !matches!(ast.tag(k), TYPEPARAM | PARAM))
}

/// Collects into `arities`, in order, for each argument of call `t` that defines a method or a
/// class (dotty's `needsHoist`), its parameter count if a lambda.
fn lambda_args(ast: &Ast, t: NodeId, arities: &mut Vec<Option<usize>>) {
    match ast.tag(t) {
        APPLY => {
            let kids = ast.kids(t);
            lambda_args(ast, kids[0], arities);
            for &a in &kids[1..] {
                let a = ast.deref(a);
                if defines(ast, a) {
                    arities.push(lambda_arity(ast, a));
                }
            }
        }
        TYPEAPPLY => lambda_args(ast, ast.kids(t)[0], arities),
        _ => {}
    }
}

/// Whether `t` has a method or class definition in it.
fn defines(ast: &Ast, t: NodeId) -> bool {
    matches!(ast.tag(t), DEFDEF | TEMPLATE) || ast.kids(t).iter().any(|&k| defines(ast, k))
}

/// The parameter count of `t` if it is a lambda of a function type: a block of a method and a
/// closure of it without a target type.
fn lambda_arity(ast: &Ast, t: NodeId) -> Option<usize> {
    let kids = ast.kids(t);
    let (&expr, stats) = kids.split_first().filter(|_| ast.tag(t) == BLOCK)?;
    let [def] = stats else { return None };
    if ast.tag(expr) != LAMBDA || ast.kids(expr).len() != 1 || ast.tag(*def) != DEFDEF {
        return None;
    }
    Some(ast.kids(*def).iter().filter(|&&k| ast.tag(k) == PARAM).count())
}
