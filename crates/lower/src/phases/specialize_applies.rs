// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SpecializeApplyMethods.scala
//! dotty's `SpecializeApplyMethods.transformTemplate`: the definition of `scala.FunctionN` gets
//! each specialized `apply$mc<R><Ps>$sp` as `this.apply(params).asInstanceOf[R]`.

use super::mixin::function_arity;
use crate::definitions::specialized_applies;
use crate::{Lower, Result, unsupported};
use names::nme;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

impl Lower<'_> {
    /// Enters the specialized `apply` methods of `class` if it is `scala.FunctionN` with them
    /// (dotty's `SpecializeApplyMethods.transformInfo`).
    pub fn enter_specialized_applies(&mut self, class: SymbolId) {
        super::same_run::enter_specialized(self, class);
    }
}

/// Adds the specialized `apply` methods to class definition `td` if it is `Function0`,
/// `Function1` or `Function2`.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Some(arity) = function_arity(l, class) else { return Ok(()) };
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { return Ok(()) };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let Ok(specialized) = specialized_applies(l.u, arity) else {
        return unsupported(l.tpd.span(td), "specialized functions");
    };
    let Some(&apply) = l.u.decls_named(class, nme::APPLY).first() else { return Ok(()) };

    let this_type = l.u.this_type(class);
    let apply_info = l.u.member_info(this_type, apply);
    let apply_result = l.u.result_type(apply_info);
    let at = l.tpd.span(template).end_pos();
    let mut stats = l.tpd.items(body).to_vec();
    for spec in specialized {
        let spec_info = l.u.info(spec);
        let result = l.u.result_type(spec_info);
        let def = l.forwarding_def(spec, at, |l, args, _| {
            let this = l.this(class, at);
            let fun = l.select(this, apply, at);
            let call = l.apply(fun, args, apply_result, at);
            l.ensure_conforms(call, result)
        });
        stats.push(l.positioned(def));
    }

    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}
