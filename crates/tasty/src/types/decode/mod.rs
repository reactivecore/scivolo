//! Decoding the types of a file's type trees, types and paths.

mod defs;
mod lambdas;
mod match_type;
mod refs;

pub use defs::param_tree_clauses;

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
    /// How many types are being decoded, one inside another.
    depth: u32,
    /// The refinements being decoded; a reference back to one decodes as `Other`.
    refining: Vec<NodeId>,
    /// The captures of the match type cases being decoded: the `BIND` tree, the case's binder
    /// and the capture's index.
    captures: Vec<(NodeId, Binder, u32)>,
}

/// How deep types nest at most; deeper ones decode as `Other`.
const MAX_DEPTH: u32 = 512;

impl<'a> Decoder<'a> {
    pub fn new(
        (file, ast, names): (FileId, &'a Ast, &'a [String]),
        by_tree: &'a FxHashMap<(FileId, NodeId), SymbolId>,
        packages: &'a FxHashMap<String, SymbolId>,
        types: &'a mut Types,
    ) -> Self {
        Self {
            file,
            ast,
            names,
            by_tree,
            packages,
            types,
            depth: 0,
            refining: Vec::new(),
            captures: Vec::new(),
        }
    }

    /// The name the node at `id` refers to.
    pub fn name(&self, id: NodeId) -> String {
        self.names.get(self.ast.node(id).a as usize).cloned().unwrap_or_default()
    }

    pub fn intern(&mut self, t: Type) -> TypeId {
        self.types.intern(t)
    }

    pub fn get(&self, t: TypeId) -> &Type {
        self.types.get(t)
    }

    fn binder(&self, id: NodeId) -> Binder {
        Binder(self.file, self.ast.node(id).addr)
    }

    /// The type of the type, type tree or path at `id`.
    pub fn ty(&mut self, id: NodeId) -> TypeId {
        if self.depth >= MAX_DEPTH {
            return self.types.intern(Type::Other(self.ast.tag(id)));
        }
        self.depth += 1;
        let t = self.decode(id);
        self.depth -= 1;
        self.types.intern(t)
    }

    /// Whether `tycon` is `scala.&` (`true`) or `scala.|` (`false`).
    fn and_or(&self, tycon: TypeId) -> Option<bool> {
        let Type::TypeRef(prefix, name) = self.types.get(tycon) else { return None };
        let scala = self.packages.get("scala");
        if !matches!(self.types.get(*prefix), Type::Package(p) if Some(p) == scala) {
            return None;
        }
        match name.as_str() {
            "&" => Some(true),
            "|" => Some(false),
            _ => None,
        }
    }

    /// The class tree of the annotation term at `id`: what its constructor call instantiates.
    fn annotation_class(&self, id: NodeId) -> Option<NodeId> {
        let ast = self.ast;
        let id = ast.deref(id);
        match ast.tag(id) {
            NEW => ast.kids(id).first().copied(),
            _ => ast.kids(id).iter().find_map(|&k| self.annotation_class(k)),
        }
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
            REFINEDtpt if self.refining.contains(&id) => Type::Other(n.tag),
            REFINEDtpt if !k.is_empty() => {
                self.refining.push(id);
                let t = self.refined(k[0], &k[1..]);
                self.refining.pop();
                self.types.get(t).clone()
            }
            IDENTtpt | IDENT | SINGLETONtpt | FLEXIBLEtype if !k.is_empty() => self.decode(k[0]),
            SUPERtype if k.len() > 1 => self.decode(k[1]),
            TYPEREF | SELECTtpt | TYPEREFin => Type::TypeRef(self.kid(id, 0), self.name(id)),
            TERMREF | SELECT | TERMREFin | SELECTin => Type::TermRef(self.kid(id, 0), self.name(id)),
            TYPEREFdirect | TERMREFdirect | TYPEREFsymbol | TERMREFsymbol => self.symbol_ref(id),
            TYPEREFpkg | TERMREFpkg => self.package_ref(id),
            THIS | QUALTHIS => Type::This(self.kid(id, 0)),
            RECthis => Type::RecThis(Binder(self.file, n.a as u32)),
            RECtype => Type::Rec(self.binder(id), self.kid(id, 0)),
            APPLIEDtype | APPLIEDtpt if !k.is_empty() => {
                let args: Vec<TypeId> = k[1..].iter().map(|&a| self.ty(a)).collect();
                let tycon = self.ty(k[0]);
                match (self.and_or(tycon), &args[..]) {
                    (Some(true), &[a, b]) => Type::And(a, b),
                    (Some(false), &[a, b]) => Type::Or(a, b, false),
                    _ => Type::Applied(tycon, args),
                }
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
            ANNOTATEDtype | ANNOTATEDtpt => {
                let annotation = match k.get(1).and_then(|&a| self.annotation_class(a)) {
                    Some(class) => self.ty(class),
                    None => self.intern(Type::Other(n.tag)),
                };
                Type::Annotated(self.kid(id, 0), annotation)
            }
            ANDtype => Type::And(self.kid(id, 0), self.kid(id, 1)),
            ORtype => Type::Or(self.kid(id, 0), self.kid(id, 1), false),
            BYNAMEtype | BYNAMEtpt => Type::ByName(self.kid(id, 0)),
            REFINEDtype => Type::Refined(self.kid(id, 0), self.name(id), self.kid(id, 1)),
            MATCHtype if k.len() > 1 => {
                let (bound, scrutinee) = (self.ty(k[0]), self.ty(k[1]));
                Type::Match { bound, scrutinee, cases: k[2..].iter().map(|&c| self.ty(c)).collect() }
            }
            MATCHCASEtype => Type::MatchCase(self.kid(id, 0), self.kid(id, 1)),
            MATCHtpt => self.match_tpt(id),
            BIND => match self.capture(id) {
                Some((binder, i)) => Type::Param(binder, i),
                None => Type::Other(n.tag),
            },
            PARAMtype => Type::Param(Binder(self.file, n.a as u32), n.b as u32),
            POLYtype | METHODtype | TYPELAMBDAtype if !k.is_empty() => self.lambda_type(id),
            LAMBDAtpt if !k.is_empty() => self.lambda_tpt(id),
            STRINGconst => Type::Str(self.name(id)),
            CLASSconst if !k.is_empty() => Type::Constant(CLASSconst, i64::from(self.ty(k[0]).0)),
            tag if crate::constant(tag, n.a).is_some() => Type::Constant(tag, n.a),
            tag => Type::Other(tag),
        }
    }
}
