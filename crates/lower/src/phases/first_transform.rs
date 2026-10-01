//! The constant folding of dotty's `FirstTransform`: a pure reference to a constant `final`
//! value becomes the constant.

use super::getters::is_const_final;
use crate::Lower;
use syntax::ast::constant::Constant;
use syntax::names::to_utf16;
use tasty::Type;
use tasty::tags::{CHARconst, DOUBLEconst, FALSEconst, FLOATconst, INTconst, LONGconst, TRUEconst};
use typer::tpd::{TNodeId, Tree};

/// Makes `t` a literal if it refers to a constant `final` value through a pure path.
pub fn const_to_literal(l: &mut Lower, t: TNodeId) {
    let pure = match *l.tpd.tree(t) {
        Tree::Ident { .. } => true,
        Tree::Select { qualifier, .. } => {
            matches!(l.tpd.tree(qualifier), Tree::Ident { .. } | Tree::This { .. })
        }
        _ => false,
    };
    let Some(s) = l.tpd.sym(t).filter(|_| pure) else { return };
    if !is_const_final(l, s) {
        return;
    }
    let info = l.u.info(s);
    let Some(value) = constant(l.u.types().get(info)) else { return };
    let node = l.tpd.get_mut(t);
    node.tree = Tree::Literal { value };
    node.tpe = Some(info);
    node.sym = None;
    if node.span.exists() {
        node.span = node.span.with_point(node.span.start());
    }
}

/// The constant of constant type `t`, if the syntax has one for it.
fn constant(t: &Type) -> Option<Constant> {
    Some(match *t {
        Type::Constant(TRUEconst, _) => Constant::Boolean(true),
        Type::Constant(FALSEconst, _) => Constant::Boolean(false),
        Type::Constant(CHARconst, b) => Constant::Char(b as u16),
        Type::Constant(INTconst, b) => Constant::Int(b as i32),
        Type::Constant(LONGconst, b) => Constant::Long(b),
        Type::Constant(FLOATconst, b) => Constant::Float(f32::from_bits(b as u32)),
        Type::Constant(DOUBLEconst, b) => Constant::Double(f64::from_bits(b as u64)),
        Type::Str(ref s) => Constant::String(to_utf16(s).into()),
        _ => return None,
    })
}
