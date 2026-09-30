//! Decoding the types of a file's type trees, types and paths.

use super::{Binder, Type, TypeId, Types};
use crate::tags::*;
use crate::{Ast, FileId, NodeId, SymbolId};
use rustc_hash::FxHashMap;

/// What decoding a type of one file reads, and the arena it adds to.
pub struct Decoder<'a> {
    file: FileId,
    ast: &'a Ast,
    names: &'a [String],
    by_tree: &'a FxHashMap<(FileId, NodeId), SymbolId>,
    packages: &'a FxHashMap<String, SymbolId>,
    types: &'a mut Types,
}

impl<'a> Decoder<'a> {
    pub fn new(
        (file, ast, names): (FileId, &'a Ast, &'a [String]),
        by_tree: &'a FxHashMap<(FileId, NodeId), SymbolId>,
        packages: &'a FxHashMap<String, SymbolId>,
        types: &'a mut Types,
    ) -> Self {
        Self { file, ast, names, by_tree, packages, types }
    }

    /// The name the node at `id` refers to.
    pub fn name(&self, id: NodeId) -> String {
        self.names.get(self.ast.node(id).a as usize).cloned().unwrap_or_default()
    }

    pub fn intern(&mut self, t: Type) -> TypeId {
        self.types.intern(t)
    }

    fn binder(&self, id: NodeId) -> Binder {
        Binder(self.file, self.ast.node(id).addr)
    }

    /// The type of the type, type tree or path at `id`.
    pub fn ty(&mut self, id: NodeId) -> TypeId {
        let t = self.decode(id);
        self.types.intern(t)
    }

    /// The type of child `i` of `id`, `Other` if it has none.
    fn kid(&mut self, id: NodeId, i: usize) -> TypeId {
        match self.ast.kids(id).get(i) {
            Some(&k) => self.ty(k),
            None => self.types.intern(Type::Other(self.ast.tag(id))),
        }
    }

    fn decode(&mut self, id: NodeId) -> Type {
        let ast = self.ast;
        let (n, k) = (ast.node(id), ast.kids(id));
        match n.tag {
            SHAREDtype | SHAREDterm => self.decode(ast.deref(id)),
            IDENTtpt | IDENT | SINGLETONtpt | FLEXIBLEtype | REFINEDtpt if !k.is_empty() => self.decode(k[0]),
            SUPERtype if k.len() > 1 => self.decode(k[1]),
            TYPEREF | SELECTtpt | TYPEREFin => Type::TypeRef(self.kid(id, 0), self.name(id)),
            TERMREF | SELECT | TERMREFin | SELECTin => Type::TermRef(self.kid(id, 0), self.name(id)),
            TYPEREFdirect | TERMREFdirect | TYPEREFsymbol | TERMREFsymbol => {
                let prefix = matches!(n.tag, TYPEREFsymbol | TERMREFsymbol).then(|| self.kid(id, 0));
                let Some(def) = ast.at(n.a as u32) else { return Type::Other(n.tag) };
                match self.by_tree.get(&(self.file, def)) {
                    Some(&s) => Type::Sym(prefix, s),
                    None => Type::Local(Binder(self.file, n.a as u32), self.name(def)),
                }
            }
            TYPEREFpkg | TERMREFpkg => {
                let name = self.name(id);
                let name = name.strip_prefix("_root_").map_or(name.as_str(), |n| n.trim_start_matches('.'));
                match self.packages.get(name) {
                    Some(&p) => Type::Package(p),
                    None => Type::PackageName(name.to_owned()),
                }
            }
            THIS | QUALTHIS => Type::This(self.kid(id, 0)),
            RECthis => Type::RecThis(Binder(self.file, n.a as u32)),
            RECtype => Type::Rec(self.binder(id), self.kid(id, 0)),
            APPLIEDtype | APPLIEDtpt if !k.is_empty() => {
                let args = k[1..].iter().map(|&a| self.ty(a)).collect();
                Type::Applied(self.ty(k[0]), args)
            }
            TYPEBOUNDS | TYPEBOUNDStpt => {
                let types: Vec<NodeId> = k.iter().copied().filter(|&c| !is_modifier(ast.tag(c))).collect();
                match types[..] {
                    [alias] if n.tag == TYPEBOUNDS => Type::Alias(self.ty(alias)),
                    [lo] => Type::Bounds(self.ty(lo), self.ty(lo)),
                    [lo, hi] => Type::Bounds(self.ty(lo), self.ty(hi)),
                    [_, _, alias, ..] => Type::Alias(self.ty(alias)),
                    [] => Type::Other(n.tag),
                }
            }
            ANNOTATEDtype | ANNOTATEDtpt => Type::Annotated(self.kid(id, 0)),
            ANDtype => Type::And(self.kid(id, 0), self.kid(id, 1)),
            ORtype => Type::Or(self.kid(id, 0), self.kid(id, 1)),
            BYNAMEtype | BYNAMEtpt => Type::ByName(self.kid(id, 0)),
            REFINEDtype => Type::Refined(self.kid(id, 0), self.name(id), self.kid(id, 1)),
            MATCHtype => Type::Match(self.kid(id, 0)),
            MATCHtpt => {
                let heads: Vec<NodeId> = k.iter().copied().filter(|&c| ast.tag(c) != CASEDEF).collect();
                match heads[..] {
                    [bound, _] => Type::Match(self.ty(bound)),
                    _ => Type::Match(self.types.intern(Type::Other(n.tag))),
                }
            }
            PARAMtype => Type::Param(Binder(self.file, n.a as u32), n.b as u32),
            POLYtype | METHODtype | TYPELAMBDAtype if !k.is_empty() => self.lambda_type(id),
            LAMBDAtpt if !k.is_empty() => {
                let (params, body) = k.split_at(k.len() - 1);
                let params = params.iter().map(|&p| (self.name(p), self.kid(p, 0))).collect();
                Type::Lambda { binder: self.binder(id), params, result: self.ty(body[0]) }
            }
            STRINGconst => Type::Str(self.name(id)),
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
