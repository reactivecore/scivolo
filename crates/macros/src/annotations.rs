//! The instance of a macro annotation the VM makes for its expansion (dotty's
//! `MacroAnnotations.callMacro` interpreting `new annot(..)`).

use crate::args::literal;
use tasty::Universe;
use typer::inlining::{MacroArg, MacroError};
use typer::tpd::{TNodeId, Tpd, Tree as T};

/// The binary name of the class of typed annotation `annotation`, the signature of the
/// constructor it calls and its arguments, all literals.
pub fn instantiation(
    u: &mut Universe,
    tpd: &Tpd,
    annotation: TNodeId,
) -> Result<(String, String, Vec<vm::MacroArg>), MacroError> {
    let refused = |what: &str| MacroError::Unsupported(what.to_owned());
    let T::Apply { fun, args } = *tpd.tree(annotation) else {
        return Err(refused("a macro annotation other than a constructor call"));
    };
    if matches!(tpd.tree(fun), T::Apply { .. }) {
        return Err(refused("a macro annotation with several argument lists"));
    }
    let ctor = tpd.sym(tpd.meth_part(fun)).filter(|&c| u.is_constructor(c));
    let ctor = ctor.ok_or_else(|| refused("a macro annotation other than a constructor call"))?;
    let class = u.symbol(ctor).owner.ok_or_else(|| refused("a macro annotation without a class"))?;
    if !u.is_static_symbol(class) {
        return Err(refused("a macro annotation of a class with an outer instance"));
    }
    let (params, _) = u.erased_method(ctor);
    let args = tpd.items(args);
    if params.len() != args.len() {
        return Err(refused("a macro annotation with erased parameters"));
    }
    let values = args.iter().zip(&params).map(|(&a, p)| match tpd.tree(a) {
        T::Literal { value } => literal(&MacroArg::Literal(value.clone()), p).flatten(),
        _ => None,
    });
    let values = values.collect::<Option<Vec<_>>>();
    let values = values.ok_or_else(|| refused("a macro annotation argument other than a literal"))?;
    Ok((u.binary_name(class), u.constructor_signature(ctor), values))
}
