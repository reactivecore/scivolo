//! The abstract methods of a functional interface (dotty's `possibleSamMethods`), whether a
//! class is one, and whether a closure implementing one needs an adapting method
//! (`TypeErasure.additionalAdaptationNeeded`).

use super::local_outer::needs_outer_if_referenced;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::{Erased, Kind, SymbolId};

/// The abstract methods of `class`, inherited ones too, but those of `Object`: of each name and
/// erased parameters, the one of the first base declaring it.
pub fn abstract_methods(l: &mut Lower, class: SymbolId) -> Vec<SymbolId> {
    let mut seen: Vec<(String, Vec<Erased>)> = Vec::new();
    let mut out = Vec::new();
    let object = l.defs.object;
    for base in l.u.linearization(class) {
        let function = l.u.full_name(base).starts_with("scala.Function");
        for d in l.u.decls(base).unwrap_or_default() {
            let s = l.u.symbol(d);
            if s.kind != Kind::Def || s.name == "<init>" || function && s.name.starts_with("apply$mc") {
                continue;
            }
            let abstract_ = match s.tree.is_some() {
                true => l.u.body(d).is_none(),
                false => l.u.is_deferred(d),
            };
            let key = (s.name.clone(), l.u.erased_method(d).0);
            if seen.contains(&key) {
                continue;
            }
            let object_decls = l.u.decl(object, &key.0).unwrap_or_default();
            let in_object = object_decls.into_iter().any(|o| l.u.erased_method(o).0 == key.1);
            seen.push(key);
            if abstract_ && base != object && !in_object {
                out.push(d);
            }
        }
    }
    out
}

/// Whether closures of single abstract method class `c` stay closures (dotty's
/// `JavaPlatform.isSam`): a trait without initialization nor fields, of superclass `Object`,
/// needing no outer instance, whose directly inherited traits have no initialization and no
/// abstract methods but its method; refused where that method's erasure differs from that of a
/// method it overrides, as then dotty's expansion would need bridges (`samExpansionNotNeeded`).
pub fn is_platform_sam(l: &mut Lower, c: SymbolId, span: Span) -> Result<bool> {
    if !l.u.is_trait(c) || !l.u.no_inits(c) || needs_outer_if_referenced(l, c) {
        return Ok(false);
    }
    let bases = l.u.linearization(c);
    let (object, any) = (l.defs.object, l.u.builtins().any);
    let superclass = bases[1..].iter().copied().find(|&b| !l.u.is_trait(b)).unwrap_or(object);
    if superclass != object && superclass != any {
        return Ok(false);
    }
    let parents: Vec<SymbolId> = l.u.parents(c).into_iter().filter_map(|p| l.u.class_symbol(p)).collect();
    let traits: Vec<SymbolId> = parents.into_iter().filter(|&p| l.u.is_trait(p)).collect();
    if !traits.iter().all(|&t| l.u.no_inits(t)) {
        return Ok(false);
    }
    let decls: Vec<SymbolId> = bases
        .iter()
        .filter(|&&b| b != object && b != any)
        .flat_map(|&b| l.u.decls(b).unwrap_or_default())
        .collect();
    if decls.into_iter().any(|d| matches!(l.u.symbol(d).kind, Kind::Val | Kind::Param)) {
        return Ok(false);
    }
    let [sam] = abstract_methods(l, c)[..] else { return Ok(false) };
    let name = l.u.symbol(sam).name.clone();
    for t in traits {
        if abstract_methods(l, t).into_iter().any(|m| l.u.symbol(m).name != name) {
            return Ok(false);
        }
    }
    let (params, result) = l.u.erased_method(sam);
    for b in bases {
        for m in l.u.decl(b, &name).unwrap_or_default() {
            let (p, r) = l.u.erased_method(m);
            if p.len() == params.len() && adaptation_needed(&params, &result, &p, &r) != (false, false) {
                return unsupported(span, "single abstract method types whose expansion needs bridges");
            }
        }
    }
    Ok(true)
}

/// Whether the semi-erased parameters `impl_params` and result `impl_result` of a closure's
/// method need adapting to those of its abstract method, `sam_params` and `sam_result`.
pub fn adaptation_needed(
    impl_params: &[Erased],
    impl_result: &Erased,
    sam_params: &[Erased],
    sam_result: &Erased,
) -> (bool, bool) {
    let class = |e: &Erased| (!matches!(e, Erased::Value(..) | Erased::Array(_))).then(|| e.clone());
    let same = |a: &Erased, b: &Erased| class(a) == class(b);
    let params = impl_params.iter().zip(sam_params).any(|(i, s)| {
        !same(i, s)
            && (matches!(i, Erased::Value(..) | Erased::Prim(_) | Erased::Unit)
                || matches!(s, Erased::Array(_)))
    });
    let result = !same(impl_result, sam_result) && matches!(impl_result, Erased::Value(..) | Erased::Unit);
    (params, result)
}
