// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Deriving.scala
//! The shape of a derived instance by the kinds of the type class's parameter and the deriving
//! class's type parameters (dotty's `deriveSingleParameter` and `deriveCanEqual`).

use super::super::Typer;
use super::Deriver;
use super::trees::{class_applied, node, param_def, param_def_of, param_ident, type_node};
use crate::defn::CAN_EQUAL;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{NameId, SymbolId, Type, TypeId, Universe};

/// How deeply nested the kinds of two type parameters may be compared.
const KIND_DEPTH: u32 = 8;

/// The parts of an instance: its type parameters, the type arguments of its evidence
/// parameters' type classes, the type class's arguments in its result type, and the type
/// lambda among them.
pub struct Instance {
    pub tparams: Vec<NodeId>,
    pub evidence: Vec<Vec<NodeId>>,
    pub args: Vec<NodeId>,
    pub lambda: Option<NodeId>,
}

/// The parts of an instance; `Ok(None)` if the class cannot be unified with the type class's
/// parameter, `Err` for a shape outside the subset.
type Shape = Result<Option<Instance>, &'static str>;

impl Deriver {
    /// Determines the parts of the instance of type class `tc`, derived by tree `d` at `span`:
    /// `Eq[C]` for `C derives Eq`, `Functor[[X] =>> C[A, X]]` for `C[A, B] derives Functor`,
    /// `CanEqual[C[A1], C[A2]]` for `C[A] derives CanEqual`; `None`, reported, if `tc` has no type
    /// parameters, the class cannot be unified with them, or its shape is not supported.
    pub fn shape(&self, t: &mut Typer<'_>, d: NodeId, tc: SymbolId, span: Span) -> Option<Instance> {
        let _ = t.u.decls(tc);
        let tc_params = t.u.type_params(tc);
        let cls_name = t.u.symbol(self.cls).name.clone();
        let tc_name = t.u.symbol(tc).name.clone();
        let cannot_be_unified = format!("{cls_name} cannot be unified with the type argument of {tc_name}");
        let instance = match tc_params.len() {
            1 => self.single_parameter(t, tc_params[0], span),
            _ if t.u.full_name(tc) == CAN_EQUAL => self.can_equal(t, &tc_params, span),
            0 => {
                t.error(d, format!("type {tc_name} in derives clause of {cls_name} has no type parameters"));
                return None;
            }
            _ => Ok(None),
        };
        match instance {
            Ok(Some(i)) => Some(i),
            Ok(None) => {
                t.error(d, cannot_be_unified);
                None
            }
            Err(what) => {
                t.unsupported(d, what);
                None
            }
        }
    }

    /// The instance for a type class of one parameter: cases (a) and (b) of dotty's
    /// `deriveSingleParameter`.
    fn single_parameter(&self, t: &mut Typer<'_>, tc_param: SymbolId, span: Span) -> Shape {
        t.complete(tc_param);
        let tc_param_info = t.u.info(tc_param);
        let tc_infos = hk_params(t.u, tc_param_info).unwrap_or_default();
        let instance_arity = tc_infos.len();
        let cls_params = class_type_params(t, self.cls);
        let cls_arity = cls_params.len();
        let aligned = instance_arity.min(cls_arity);
        let cls_infos: Vec<TypeId> = cls_params.iter().map(|&p| t.u.info(p)).collect();
        let same = cls_infos[cls_arity - aligned..]
            .iter()
            .zip(&tc_infos[instance_arity - aligned..])
            .all(|(&a, (_, b))| same_kind(t.u, a, *b, KIND_DEPTH));
        let class = crate::defn::type_ref(t.u, self.cls);

        if (instance_arity == cls_arity || instance_arity > 0) && same {
            let derived = &cls_params[..cls_arity - aligned];
            let tparams = derived.iter().map(|&p| param_def_of(t, p, span)).collect::<Result<_, _>>()?;
            let arg = if instance_arity == cls_arity {
                type_node(t, class, span)
            } else {
                partial_application(t, class, derived, &tc_infos, aligned, span)?
            };
            let lambda = (instance_arity != cls_arity).then_some(arg);
            return Ok(Some(Instance { tparams, evidence: Vec::new(), args: vec![arg], lambda }));
        }

        if instance_arity == 0 && cls_infos.iter().all(|&i| hk_params(t.u, i).is_none()) {
            let tparams = cls_params.iter().map(|&p| param_def_of(t, p, span)).collect::<Result<_, _>>()?;
            let args: Vec<NodeId> = cls_params.iter().map(|&p| param_ident(t, p, span)).collect();
            let evidence = cls_params.iter().map(|&p| vec![param_ident(t, p, span)]).collect();
            let arg = class_applied(t, class, args, span);
            return Ok(Some(Instance { tparams, evidence, args: vec![arg], lambda: None }));
        }
        Ok(None)
    }

    /// The `CanEqual` instance of the class: a pair of type parameters per parameter of the
    /// class, and evidence for the pairs of kind `*` (dotty's `deriveCanEqual`).
    fn can_equal(&self, t: &mut Typer<'_>, tc_params: &[SymbolId], span: Span) -> Shape {
        let cls_params = class_type_params(t, self.cls);
        let mut rows: Vec<(Vec<String>, bool)> = Vec::new();
        let mut tparams = Vec::new();
        for &p in &cls_params {
            let info = t.u.info(p);
            let mut row = Vec::new();
            for &tcp in tc_params {
                let name = format!("{}_$_{}", t.u.symbol(p).name, t.u.symbol(tcp).name);
                tparams.push(param_def(t, &name, info, span)?);
                row.push(name);
            }
            rows.push((row, hk_params(t.u, info).is_none()));
        }

        let ident = |t: &mut Typer<'_>, n: &str| {
            let name = TName { name: t.name(n), is_type: true };
            node(t, Tree::Ident { name }, span)
        };
        let evidence = rows
            .iter()
            .filter(|(_, first)| *first)
            .map(|(row, _)| row.iter().map(|n| ident(t, n)).collect())
            .collect();
        let class = crate::defn::type_ref(t.u, self.cls);
        let args = (0..tc_params.len())
            .map(|i| {
                let args = rows.iter().map(|(row, _)| ident(t, &row[i])).collect();
                class_applied(t, class, args, span)
            })
            .collect();
        Ok(Some(Instance { tparams, evidence, args, lambda: None }))
    }
}

/// `[X1, .., Xn] =>> class[A, .., Xk, .., Xn]`: the type lambda over the parameters `tc_infos`
/// of the type class's parameter, applying `class` to its own parameters `derived` and the last
/// `aligned` parameters of the lambda.
fn partial_application(
    t: &mut Typer<'_>,
    class: TypeId,
    derived: &[SymbolId],
    tc_infos: &[(NameId, TypeId)],
    aligned: usize,
    span: Span,
) -> Result<NodeId, &'static str> {
    let mut args: Vec<NodeId> = derived.iter().map(|&p| param_ident(t, p, span)).collect();
    let lambda_names: Vec<String> = tc_infos.iter().map(|&(n, _)| t.u.name(n).to_owned()).collect();
    if derived.iter().any(|&p| lambda_names.contains(&t.u.symbol(p).name)) {
        return Err("derives clause whose type lambda shadows a type parameter");
    }
    let lparams = (lambda_names.iter().zip(tc_infos))
        .map(|(n, &(_, b))| param_def(t, n, b, span))
        .collect::<Result<Vec<_>, _>>()?;
    for n in &lambda_names[tc_infos.len() - aligned..] {
        let name = TName { name: t.name(n), is_type: true };
        args.push(node(t, Tree::Ident { name }, span));
    }
    let body = class_applied(t, class, args, span);
    let tparams = t.ast.list(&lparams);
    Ok(node(t, Tree::LambdaTypeTree { tparams, body }, span))
}

/// The type parameters of class `cls`, completed.
fn class_type_params(t: &mut Typer<'_>, cls: SymbolId) -> Vec<SymbolId> {
    t.complete(cls);
    let _ = t.u.decls(cls);
    let params = t.u.type_params(cls);
    params.iter().for_each(|&p| t.complete(p));
    params
}

/// The parameters of higher-kinded type `info`, a type lambda or bounds over one.
fn hk_params(u: &Universe, info: TypeId) -> Option<Vec<(NameId, TypeId)>> {
    match u.ty(info) {
        Type::Lambda { params, .. } => Some(params.clone()),
        &Type::Bounds(_, hi) => hk_params(u, hi),
        _ => None,
    }
}

/// Whether type parameter infos `a` and `b` have the same kind, compared `depth` levels deep
/// (dotty's `hasSameKindAs`).
fn same_kind(u: &Universe, a: TypeId, b: TypeId, depth: u32) -> bool {
    match (hk_params(u, a), hk_params(u, b)) {
        (None, None) => true,
        (Some(x), Some(y)) if depth > 0 && x.len() == y.len() => {
            x.iter().zip(&y).all(|((_, a), (_, b))| same_kind(u, *a, *b, depth - 1))
        }
        _ => false,
    }
}
