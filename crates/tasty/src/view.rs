use crate::names::NameRef;
use crate::tags::*;
use crate::tree::{Ast, NodeId};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Const {
    Unit,
    Bool(bool),
    Null,
    Byte(i8),
    Short(i16),
    Char(char),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Str(NameRef),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Parts<'t> {
    pub params: &'t [NodeId],
    pub body: &'t [NodeId],
    pub mods: &'t [NodeId],
}

/// Named interpretation of the tags an interpreter cares about; everything else is `Other`.
#[derive(Debug, Clone, Copy)]
pub enum View<'t> {
    Package { pid: NodeId, stats: &'t [NodeId] },
    TypeDef { name: NameRef, rhs: NodeId, is_class: bool, mods: &'t [NodeId] },
    Template(Parts<'t>),
    DefDef { name: NameRef, parts: Parts<'t> },
    ValDef { name: NameRef, tpt: NodeId, rhs: Option<NodeId>, mods: &'t [NodeId] },
    Param { name: NameRef, tpt: NodeId, mods: &'t [NodeId] },
    Ident { name: NameRef, tpe: NodeId },
    Select { name: NameRef, qual: NodeId },
    Apply { fun: NodeId, args: &'t [NodeId] },
    TypeApply { fun: NodeId, targs: &'t [NodeId] },
    Block { stats: &'t [NodeId], expr: NodeId },
    If { inline: bool, cond: NodeId, then: NodeId, els: NodeId },
    Match { prefix: Option<u8>, sel: Option<NodeId>, cases: &'t [NodeId] },
    CaseDef { pat: NodeId, rhs: NodeId, guard: Option<NodeId> },
    Inlined { expr: NodeId, call: Option<NodeId>, bindings: &'t [NodeId] },
    Lambda { meth: NodeId, target: Option<NodeId> },
    Const(Const),
    Shared(u32),
    Other,
}

pub fn view(ast: &Ast, id: NodeId) -> View<'_> {
    let n = ast.node(id);
    let k = ast.kids(id);
    let name = NameRef(n.a as u32);
    match n.tag {
        PACKAGE if !k.is_empty() => View::Package { pid: k[0], stats: &k[1..] },
        TYPEDEF if !k.is_empty() => {
            View::TypeDef { name, rhs: k[0], is_class: ast.tag(k[0]) == TEMPLATE, mods: &k[1..] }
        }
        TEMPLATE => View::Template(split(ast, k)),
        DEFDEF => View::DefDef { name, parts: split(ast, k) },
        VALDEF if !k.is_empty() => {
            let m = mods_start(ast, k);
            View::ValDef { name, tpt: k[0], rhs: (m > 1).then(|| k[1]), mods: &k[m..] }
        }
        PARAM | TYPEPARAM if !k.is_empty() => View::Param { name, tpt: k[0], mods: &k[1..] },
        IDENT | IDENTtpt => View::Ident { name, tpe: k[0] },
        SELECT | SELECTtpt => View::Select { name, qual: k[0] },
        APPLY if !k.is_empty() => View::Apply { fun: k[0], args: &k[1..] },
        TYPEAPPLY if !k.is_empty() => View::TypeApply { fun: k[0], targs: &k[1..] },
        BLOCK if !k.is_empty() => View::Block { expr: k[0], stats: &k[1..] },
        IF if k.len() == 4 => View::If { inline: true, cond: k[1], then: k[2], els: k[3] },
        IF if k.len() == 3 => View::If { inline: false, cond: k[0], then: k[1], els: k[2] },
        MATCH => {
            let prefix =
                k.first().map(|&p| ast.tag(p)).filter(|t| matches!(*t, IMPLICIT | INLINE | SUBMATCH));
            let rest = if prefix.is_some() { &k[1..] } else { k };
            let (sel, cases) = match (prefix, rest.split_first()) {
                (Some(IMPLICIT), _) | (_, None) => (None, rest),
                (_, Some((s, c))) => (Some(*s), c),
            };
            View::Match { prefix, sel, cases }
        }
        CASEDEF if k.len() >= 2 => View::CaseDef { pat: k[0], rhs: k[1], guard: k.get(2).copied() },
        INLINED if !k.is_empty() => {
            let call = k.get(1).copied().filter(|&c| !matches!(ast.tag(ast.deref(c)), VALDEF | DEFDEF));
            let skip = 1 + usize::from(call.is_some());
            View::Inlined { expr: k[0], call, bindings: &k[skip..] }
        }
        LAMBDA if !k.is_empty() => View::Lambda { meth: k[0], target: k.get(1).copied() },
        SHAREDterm | SHAREDtype => View::Shared(n.a as u32),
        _ => constant(n.tag, n.a).map_or(View::Other, View::Const),
    }
}

pub fn constant(tag: u8, a: i64) -> Option<Const> {
    Some(match tag {
        UNITconst => Const::Unit,
        TRUEconst => Const::Bool(true),
        FALSEconst => Const::Bool(false),
        NULLconst => Const::Null,
        BYTEconst => Const::Byte(a as i8),
        SHORTconst => Const::Short(a as i16),
        CHARconst => Const::Char(char::from_u32(a as u32).unwrap_or('\u{FFFD}')),
        INTconst => Const::Int(a as i32),
        LONGconst => Const::Long(a),
        FLOATconst => Const::Float(f32::from_bits(a as i32 as u32)),
        DOUBLEconst => Const::Double(f64::from_bits(a as u64)),
        STRINGconst => Const::Str(NameRef(a as u32)),
        _ => return None,
    })
}

fn mods_start(ast: &Ast, k: &[NodeId]) -> usize {
    k.iter().rposition(|&c| !is_modifier(ast.tag(c))).map_or(0, |i| i + 1)
}

/// Splits DEFDEF/TEMPLATE children into leading params, body (type/parents/stats) and trailing modifiers.
fn split<'t>(ast: &Ast, k: &'t [NodeId]) -> Parts<'t> {
    let p = k.iter().position(|&c| !is_param(ast.tag(c))).unwrap_or(k.len());
    let m = mods_start(ast, k).max(p);
    Parts { params: &k[..p], body: &k[p..m], mods: &k[m..] }
}
