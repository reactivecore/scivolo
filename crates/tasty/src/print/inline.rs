use crate::names::{NameRef, Names};
use crate::tags::*;
use crate::tree::{Ast, NodeId};
use crate::view::{Const, constant};

/// Tags rendered on one line: types, paths, type trees, constants and modifiers.
pub fn is_inline(tag: u8) -> bool {
    matches!(
        tag,
        NAMEref
            | 2..=59
            | SHAREDtype
            | TERMREFdirect
            | TYPEREFdirect
            | TERMREFpkg
            | TYPEREFpkg
            | RECthis
            | BYTEconst..=STRINGconst
            | THIS
            | QUALTHIS
            | CLASSconst
            | BYNAMEtype
            | BYNAMEtpt
            | PRIVATEqualified
            | PROTECTEDqualified
            | RECtype
            | SINGLETONtpt
            | IDENTtpt
            | SELECTtpt
            | TERMREFsymbol
            | TERMREF
            | TYPEREFsymbol
            | TYPEREF
            | ANNOTATEDtype
            | SUPERtype
            | REFINEDtype
            | APPLIEDtype
            | APPLIEDtpt
            | TYPEBOUNDS
            | TYPEBOUNDStpt
            | ANDtype
            | ORtype
            | POLYtype
            | TYPELAMBDAtype
            | PARAMtype
            | TERMREFin
            | TYPEREFin
            | METHODtype
            | MATCHtype
            | MATCHCASEtype
            | FLEXIBLEtype
    )
}

pub struct Inline<'x> {
    pub ast: &'x Ast,
    pub names: &'x Names<'x>,
}

impl Inline<'_> {
    pub fn name(&self, a: i64) -> String {
        self.names.show(NameRef(a as u32))
    }

    /// Name of the definition at an AST address, suffixed with the address.
    pub fn sym(&self, addr: i64) -> String {
        match self.ast.at(addr as u32) {
            Some(d) if leading_nats(self.ast.tag(d)) > 0 => {
                format!("{}@{addr}", self.name(self.ast.node(d).a))
            }
            _ => format!("@{addr}"),
        }
    }

    pub fn render(&self, id: NodeId) -> String {
        let n = self.ast.node(id);
        let k = self.ast.kids(id);
        let r = |i: usize| k.get(i).map_or_else(|| "?".into(), |&c| self.render(c));
        let all = |from: usize, sep: &str| {
            k.iter().skip(from).map(|&c| self.render(c)).collect::<Vec<_>>().join(sep)
        };
        if let Some(c) = constant(n.tag, n.a) {
            return self.constant(c);
        }
        match n.tag {
            NAMEref => self.name(n.a),
            SHAREDtype | SHAREDterm => self.render(self.ast.deref(id)),
            TERMREFdirect | TYPEREFdirect => self.sym(n.a),
            TERMREFpkg | TYPEREFpkg => self.name(n.a),
            RECthis => format!("{}.this", self.sym(n.a)),
            THIS => format!("{}.this", r(0)),
            QUALTHIS => format!("{}.this", r(0)),
            CLASSconst => format!("classOf[{}]", r(0)),
            BYNAMEtype | BYNAMEtpt => format!("=> {}", r(0)),
            RECtype => format!("rec({})", r(0)),
            SINGLETONtpt => format!("{}.type", r(0)),
            IDENTtpt => self.name(n.a),
            TERMREF | TYPEREF | SELECTtpt => format!("{}.{}", r(0), self.name(n.a)),
            TERMREFsymbol | TYPEREFsymbol => format!("{}.{}", r(0), self.sym(n.a)),
            TERMREFin | TYPEREFin => format!("{}.{}(in {})", r(0), self.name(n.a), r(1)),
            APPLIEDtype | APPLIEDtpt => format!("{}[{}]", r(0), all(1, ", ")),
            ANDtype => format!("({} & {})", r(0), r(1)),
            ORtype => format!("({} | {})", r(0), r(1)),
            TYPEBOUNDS | TYPEBOUNDStpt if k.len() == 1 => format!("= {}", r(0)),
            TYPEBOUNDS | TYPEBOUNDStpt => format!(">: {} <: {}", r(0), all(1, " ")),
            ANNOTATEDtype => format!("{} @{}", r(0), r(1)),
            REFINEDtype => format!("{} {{ {}: {} }}", r(0), self.name(n.a), r(1)),
            SUPERtype => format!("{}.super[{}]", r(0), r(1)),
            PARAMtype => format!("{}#{}", self.param_name(n.a, n.b), n.a),
            POLYtype | TYPELAMBDAtype | METHODtype => self.methodic(n.tag, k),
            FLEXIBLEtype => format!("({})?", r(0)),
            MATCHtype => format!("{} match {{ {} }}", r(1), all(2, "; ")),
            MATCHCASEtype => format!("case {} => {}", r(0), r(1)),
            IDENT => self.name(n.a),
            SELECT | SELECTin => format!("{}.{}", r(0), self.name(n.a)),
            NEW => format!("new {}", r(0)),
            APPLY => format!("{}({})", r(0), all(1, ", ")),
            TYPEAPPLY => format!("{}[{}]", r(0), all(1, ", ")),
            ANNOTATION => format!("@{}", r(1)),
            PRIVATEqualified => format!("private[{}]", r(0)),
            PROTECTEDqualified => format!("protected[{}]", r(0)),
            t if t < FIRST_NAT => name(t).to_lowercase(),
            t if k.is_empty() => name(t).into(),
            t => format!("{}({})", name(t), all(0, ", ")),
        }
    }

    fn param_name(&self, binder: i64, num: i64) -> String {
        let Some(b) = self.ast.at(binder as u32) else { return format!("?{num}") };
        let ns: Vec<_> = self.ast.kids(b).iter().filter(|&&c| self.ast.tag(c) == NAMEref).collect();
        ns.get(num as usize).map_or_else(|| format!("?{num}"), |&&c| self.name(self.ast.node(c).a))
    }

    fn methodic(&self, tag: u8, k: &[NodeId]) -> String {
        let mut params = Vec::new();
        let mut mods = Vec::new();
        let mut i = 1;
        while i < k.len() {
            if k.get(i + 1).is_some_and(|&c| self.ast.tag(c) == NAMEref) {
                params.push(format!("{}: {}", self.render(k[i + 1]), self.render(k[i])));
                i += 2;
            } else {
                mods.push(self.render(k[i]));
                i += 1;
            }
        }
        let body = k.first().map_or_else(String::new, |&c| self.render(c));
        let m = if mods.is_empty() { String::new() } else { format!("{} ", mods.join(" ")) };
        match tag {
            METHODtype => format!("({m}{}){body}", params.join(", ")),
            POLYtype => format!("[{}]{body}", params.join(", ")),
            _ => format!("[{}] =>> {body}", params.join(", ")),
        }
    }

    pub fn constant(&self, c: Const) -> String {
        match c {
            Const::Unit => "()".into(),
            Const::Bool(b) => b.to_string(),
            Const::Null => "null".into(),
            Const::Byte(v) => format!("{v}.toByte"),
            Const::Short(v) => format!("{v}.toShort"),
            Const::Char(v) => format!("{v:?}"),
            Const::Int(v) => v.to_string(),
            Const::Long(v) => format!("{v}L"),
            Const::Float(v) => format!("{v}f"),
            Const::Double(v) => format!("{v}d"),
            Const::Str(r) => format!("{:?}", self.names.show(r)),
        }
    }
}
