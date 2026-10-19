// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Deriving.scala
//! The shape of a derived instance by the kinds of the type class's parameter and the deriving
//! class's type parameters (dotty's `deriveSingleParameter` and `deriveCanEqual`).

use super::Typer;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{SymbolId, Type, TypeId};

/// The parts of an instance: its type parameters, the type arguments of its evidence
/// parameters' type classes, the type class's arguments in its result type, and the type
/// lambda among them.
pub struct Instance {
    pub tparams: Vec<NodeId>,
    pub evidence: Vec<Vec<NodeId>>,
    pub args: Vec<NodeId>,
    pub lambda: Option<NodeId>,
}

impl Typer<'_> {
    /// The type class instance for a type class of one parameter: cases (a) and (b) of dotty's
    /// `deriveSingleParameter`; `None` if `cls` cannot be unified with its parameter.
    pub fn derive_single_parameter(
        &mut self,
        cls: SymbolId,
        tc_param: SymbolId,
        span: Span,
    ) -> Result<Option<Instance>, &'static str> {
        self.complete(tc_param);
        let tc_param_info = self.u.info(tc_param);
        let tc_infos = self.hk_params(tc_param_info).unwrap_or_default();
        let instance_arity = tc_infos.len();
        let cls_params = self.class_type_params(cls);
        let cls_arity = cls_params.len();
        let aligned = instance_arity.min(cls_arity);
        let cls_infos: Vec<TypeId> = cls_params.iter().map(|&p| self.u.info(p)).collect();
        let same = cls_infos[cls_arity - aligned..]
            .iter()
            .zip(&tc_infos[instance_arity - aligned..])
            .all(|(&a, (_, b))| self.same_kind(a, *b, 8));
        let class = crate::defn::type_ref(self.u, cls);
        if (instance_arity == cls_arity || instance_arity > 0) && same {
            let derived = &cls_params[..cls_arity - aligned];
            let tparams = derived.iter().map(|&p| self.param_def_of(p, span)).collect::<Result<_, _>>()?;
            let arg = if instance_arity == cls_arity {
                self.derived_type(class, span)
            } else {
                let mut args: Vec<NodeId> = derived.iter().map(|&p| self.param_ident(p, span)).collect();
                let lambda_names: Vec<String> = tc_infos.iter().map(|(n, _)| n.clone()).collect();
                if derived.iter().any(|&p| lambda_names.contains(&self.u.symbol(p).name)) {
                    return Err("derives clause whose type lambda shadows a type parameter");
                }
                let lparams = tc_infos
                    .iter()
                    .map(|(n, b)| self.param_def(n, *b, span))
                    .collect::<Result<Vec<_>, _>>()?;
                for n in &lambda_names[instance_arity - aligned..] {
                    let name = TName { name: self.name(n), is_type: true };
                    args.push(self.derived_node(Tree::Ident { name }, span));
                }
                let body = self.class_applied(class, args, span);
                let tparams = self.ast.list(&lparams);
                self.derived_node(Tree::LambdaTypeTree { tparams, body }, span)
            };
            let lambda = (instance_arity != cls_arity).then_some(arg);
            return Ok(Some(Instance { tparams, evidence: Vec::new(), args: vec![arg], lambda }));
        }
        if instance_arity == 0 && cls_infos.iter().all(|&i| self.hk_params(i).is_none()) {
            let tparams = cls_params.iter().map(|&p| self.param_def_of(p, span)).collect::<Result<_, _>>()?;
            let args: Vec<NodeId> = cls_params.iter().map(|&p| self.param_ident(p, span)).collect();
            let evidence = cls_params.iter().map(|&p| vec![self.param_ident(p, span)]).collect();
            let arg = self.class_applied(class, args, span);
            return Ok(Some(Instance { tparams, evidence, args: vec![arg], lambda: None }));
        }
        Ok(None)
    }

    /// The `CanEqual` instance of `cls`: a pair of type parameters per parameter of `cls`, and
    /// evidence for the pairs of kind `*` (dotty's `deriveCanEqual`).
    pub fn derive_can_equal(
        &mut self,
        cls: SymbolId,
        tc_params: &[SymbolId],
        span: Span,
    ) -> Result<Option<Instance>, &'static str> {
        let cls_params = self.class_type_params(cls);
        let mut rows: Vec<(Vec<String>, bool)> = Vec::new();
        let mut tparams = Vec::new();
        for &p in &cls_params {
            let info = self.u.info(p);
            let mut row = Vec::new();
            for &tcp in tc_params {
                let name = format!("{}_$_{}", self.u.symbol(p).name, self.u.symbol(tcp).name);
                tparams.push(self.param_def(&name, info, span)?);
                row.push(name);
            }
            rows.push((row, self.hk_params(info).is_none()));
        }
        let ident = |t: &mut Self, n: &str| {
            let name = TName { name: t.name(n), is_type: true };
            t.derived_node(Tree::Ident { name }, span)
        };
        let evidence = rows
            .iter()
            .filter(|(_, first)| *first)
            .map(|(row, _)| row.iter().map(|n| ident(self, n)).collect())
            .collect();
        let class = crate::defn::type_ref(self.u, cls);
        let args = (0..tc_params.len())
            .map(|i| {
                let args = rows.iter().map(|(row, _)| ident(self, &row[i])).collect();
                self.class_applied(class, args, span)
            })
            .collect();
        Ok(Some(Instance { tparams, evidence, args, lambda: None }))
    }

    /// The type parameters of class `cls`, completed.
    fn class_type_params(&mut self, cls: SymbolId) -> Vec<SymbolId> {
        self.complete(cls);
        let _ = self.u.decls(cls);
        let params = self.u.type_params(cls);
        params.iter().for_each(|&p| self.complete(p));
        params
    }

    /// The parameters of higher-kinded type `info`, a type lambda or bounds over one.
    fn hk_params(&mut self, info: TypeId) -> Option<Vec<(String, TypeId)>> {
        match self.u.types().get(info).clone() {
            Type::Lambda { params, .. } => Some(params),
            Type::Bounds(_, hi) => self.hk_params(hi),
            _ => None,
        }
    }

    /// Whether type parameter infos `a` and `b` have the same kind (dotty's `hasSameKindAs`).
    fn same_kind(&mut self, a: TypeId, b: TypeId, depth: u32) -> bool {
        match (self.hk_params(a), self.hk_params(b)) {
            (None, None) => true,
            (Some(x), Some(y)) if depth > 0 && x.len() == y.len() => {
                x.iter().zip(&y).all(|((_, a), (_, b))| self.same_kind(*a, *b, depth - 1))
            }
            _ => false,
        }
    }
}
