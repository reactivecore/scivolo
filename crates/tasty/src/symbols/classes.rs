//! Classes' parents, linearization and inherited members.

use super::{FileId, Kind, SymbolId, Universe};
use crate::tags::*;
use crate::types::{Decoder, Type, TypeId};
use crate::{Ast, NodeId};

impl Universe {
    /// The parent types of class `c`, in declaration order.
    pub fn parents(&mut self, c: SymbolId) -> Vec<TypeId> {
        if let Some(p) = self.parents.get(&c) {
            return p.clone();
        }
        let parents = match self.symbol(c).tree {
            Some((file, id)) if self.symbol(c).kind == Kind::Class => self.decode_parents(file, id),
            _ => Vec::new(),
        };
        self.parents.insert(c, parents.clone());
        parents
    }

    fn decode_parents(&mut self, file: FileId, typedef: NodeId) -> Vec<TypeId> {
        let Universe { files, by_tree, packages, types, .. } = self;
        let loaded = &files[file.0 as usize];
        let ast = &loaded.ast;
        let Some(&template) = ast.kids(typedef).first() else { return Vec::new() };
        let mut d = Decoder::new((file, ast, &loaded.names), by_tree, packages, types);
        ast.kids(template)
            .iter()
            .copied()
            .skip_while(|&k| matches!(ast.tag(k), TYPEPARAM | PARAM | EMPTYCLAUSE))
            .take_while(|&k| !matches!(ast.tag(k), SELFDEF | SPLITCLAUSE | DEFDEF | VALDEF | TYPEDEF))
            .map(|parent| parent_type(&mut d, ast, parent))
            .collect()
    }

    /// Class `c` and its base classes in linearization order, `c` first.
    pub fn linearization(&mut self, c: SymbolId) -> Vec<SymbolId> {
        if let Some(l) = self.linearizations.get(&c) {
            return l.clone();
        }
        self.linearizations.insert(c, Vec::new());
        let mut lin: Vec<SymbolId> = Vec::new();
        for parent in self.parents(c) {
            let Some(p) = self.class_symbol(parent) else { continue };
            let mut plin: Vec<SymbolId> =
                self.linearization(p).into_iter().filter(|b| !lin.contains(b)).collect();
            plin.append(&mut lin);
            lin = plin;
        }
        lin.insert(0, c);
        self.linearizations.insert(c, lin.clone());
        lin
    }

    /// Whether class `c` is `base` or derives from it.
    pub fn derives_from(&mut self, c: SymbolId, base: SymbolId) -> bool {
        c == base || self.linearization(c).contains(&base)
    }

    /// The members of class `c` named `name`: the declarations of the first class in its
    /// linearization that declares the name.
    pub fn member(&mut self, c: SymbolId, name: &str) -> Vec<SymbolId> {
        for base in self.linearization(c) {
            let found = self.decl(base, name).unwrap_or_default();
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }

    /// The class type `t` refers to, if the universe has it.
    pub fn class_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        match self.types().get(t).clone() {
            Type::Sym(_, s) if self.symbol(s).kind == Kind::Class => Some(s),
            Type::TypeRef(p, n) => match self.resolve(p, &n, true)? {
                s if self.symbol(s).kind == Kind::Class => Some(s),
                s => {
                    let info = self.info(s);
                    self.class_symbol(info)
                }
            },
            Type::Applied(tycon, _) | Type::Annotated(tycon) | Type::Alias(tycon) => self.class_symbol(tycon),
            _ => None,
        }
    }
}

/// The type of a parent in a template: a type tree, or the class of a constructor call such as
/// `new C[T](x)`, whose `new` carries the type arguments.
fn parent_type(d: &mut Decoder, ast: &Ast, id: NodeId) -> TypeId {
    let id = ast.deref(id);
    let k = ast.kids(id);
    match ast.tag(id) {
        APPLY | TYPEAPPLY | SELECT | SELECTin if !k.is_empty() => parent_type(d, ast, k[0]),
        NEW if !k.is_empty() => d.ty(k[0]),
        _ => d.ty(id),
    }
}
