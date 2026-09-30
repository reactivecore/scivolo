//! Decoding the types of a file's type trees, types and paths.

use super::{Binder, Type, TypeId, Types};
use crate::tags::*;
use crate::{Ast, FileId, NameRef, Names, NodeId, SymbolId};
use rustc_hash::FxHashMap;

/// What decoding a type of one file reads, and the arena it adds to.
pub struct Decoder<'a> {
    pub file: FileId,
    pub ast: &'a Ast,
    pub names: &'a Names<'a>,
    pub by_tree: &'a FxHashMap<(FileId, NodeId), SymbolId>,
    pub packages: &'a FxHashMap<String, SymbolId>,
    pub types: &'a mut Types,
}

impl Decoder<'_> {
    fn name(&self, id: NodeId) -> String {
        self.names.show(NameRef(self.ast.node(id).a as u32))
    }

    fn binder(&self, id: NodeId) -> Binder {
        Binder(self.file, self.ast.node(id).addr)
    }

    /// The type of the type, type tree or path at `id`.
    pub fn ty(&mut self, id: NodeId) -> TypeId {
        let t = self.decode(id);
        self.types.intern(t)
    }

    fn decode(&mut self, id: NodeId) -> Type {
        let ast = self.ast;
        let (n, k) = (ast.node(id), ast.kids(id));
        match n.tag {
            SHAREDtype | SHAREDterm => self.decode(ast.deref(id)),
            IDENTtpt | IDENT | SINGLETONtpt | FLEXIBLEtype | REFINEDtpt => self.decode(k[0]),
            SUPERtype => self.decode(k[1]),
            TYPEREF | SELECTtpt | TYPEREFin => Type::TypeRef(self.ty(k[0]), self.name(id)),
            TERMREF | SELECT | TERMREFin => Type::TermRef(self.ty(k[0]), self.name(id)),
            TYPEREFdirect | TERMREFdirect | TYPEREFsymbol | TERMREFsymbol => {
                let prefix = matches!(n.tag, TYPEREFsymbol | TERMREFsymbol).then(|| self.ty(k[0]));
                let Some(def) = ast.at(n.a as u32) else { return Type::Other(n.tag) };
                match self.by_tree.get(&(self.file, def)) {
                    Some(&s) => Type::Sym(prefix, s),
                    None => Type::Local(Binder(self.file, n.a as u32), self.name(def)),
                }
            }
            TYPEREFpkg | TERMREFpkg => match self.packages.get(&self.name(id)) {
                Some(&p) => Type::Package(p),
                None => Type::PackageName(self.name(id)),
            },
            THIS | QUALTHIS => Type::This(self.ty(k[0])),
            RECthis => Type::RecThis(Binder(self.file, n.a as u32)),
            RECtype => Type::Rec(self.binder(id), self.ty(k[0])),
            APPLIEDtype | APPLIEDtpt => {
                let args = k[1..].iter().map(|&a| self.ty(a)).collect();
                Type::Applied(self.ty(k[0]), args)
            }
            TYPEBOUNDS | TYPEBOUNDStpt => {
                let types: Vec<NodeId> = k.iter().copied().filter(|&c| !is_modifier(ast.tag(c))).collect();
                match types[..] {
                    [alias] if n.tag == TYPEBOUNDS => Type::Alias(self.ty(alias)),
                    [lo] => Type::Bounds(self.ty(lo), self.ty(lo)),
                    [lo, hi, ..] => Type::Bounds(self.ty(lo), self.ty(hi)),
                    [] => Type::Other(n.tag),
                }
            }
            ANNOTATEDtype | ANNOTATEDtpt => Type::Annotated(self.ty(k[0])),
            ANDtype => Type::And(self.ty(k[0]), self.ty(k[1])),
            ORtype => Type::Or(self.ty(k[0]), self.ty(k[1])),
            BYNAMEtype | BYNAMEtpt => Type::ByName(self.ty(k[0])),
            REFINEDtype => Type::Refined(self.ty(k[0]), self.name(id), self.ty(k[1])),
            PARAMtype => Type::Param(Binder(self.file, n.a as u32), n.b as u32),
            POLYtype | METHODtype | TYPELAMBDAtype => self.lambda_type(id),
            LAMBDAtpt => {
                let (params, body) = k.split_at(k.len() - 1);
                let params = params.iter().map(|&p| (self.name(p), self.ty(ast.kids(p)[0]))).collect();
                Type::Lambda { binder: self.binder(id), params, result: self.ty(body[0]) }
            }
            tag if crate::constant(tag, n.a).is_some() => Type::Constant(tag, n.a),
            tag => Type::Other(tag),
        }
    }

    /// A `POLYtype`, `METHODtype` or `TYPELAMBDAtype`: its result, then each parameter's type
    /// and name, then for a method its modifiers.
    fn lambda_type(&mut self, id: NodeId) -> Type {
        let (tag, k) = (self.ast.tag(id), self.ast.kids(id));
        let result = self.ty(k[0]);
        let mut params = Vec::new();
        let mut implicit = false;
        let mut rest = k[1..].iter().copied();
        while let Some(c) = rest.next() {
            match self.ast.tag(c) {
                IMPLICIT | GIVEN => implicit = true,
                t if is_modifier(t) => {}
                _ => {
                    let ty = self.ty(c);
                    let name = rest.next().map_or_else(String::new, |n| self.name(n));
                    params.push((name, ty));
                }
            }
        }
        let binder = self.binder(id);
        match tag {
            POLYtype => Type::Poly { binder, params, result },
            METHODtype => Type::Method { binder, params, result, implicit },
            _ => Type::Lambda { binder, params, result },
        }
    }
}
