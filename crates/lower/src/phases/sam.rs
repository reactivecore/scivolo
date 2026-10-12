//! The abstract methods of a functional interface (dotty's `possibleSamMethods`) and whether a
//! closure implementing one needs an adapting method (`TypeErasure.additionalAdaptationNeeded`).

use crate::Lower;
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
