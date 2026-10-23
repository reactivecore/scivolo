// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
//! Paths, types and constants read as trees (dotty's `readPathTree` and `tpd.ref`).

use super::{Refusal, Unpickler};
use crate::defn::type_ref;
use crate::tpd::{Build, TNodeId, Tree as T};
use syntax::ast::constant::Constant;
use syntax::ast::{Span, TName};
use tasty::tags::*;
use tasty::{Const, Kind, NodeId, SymbolId, Type, TypeId};

impl Unpickler<'_> {
    /// A path, a type or a constant as a tree.
    pub fn path(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        if let Some(value) = self.constant(id) {
            let unit = self.class_symbol("scala.Unit")?;
            return Ok(Build::new(self.tpd, self.u, self.names, unit).literal(value, Span::NONE));
        }
        match self.tag(id) {
            SHAREDtype => {
                let target = self.ast().deref(id);
                self.path(target)
            }
            TYPEREFdirect | TYPEREFsymbol | TYPEREFpkg | TYPEREF | TYPEREFin | APPLIEDtype => {
                let tpe = self.ty(id);
                Ok(self.bare(T::TypeTree { inferred: false }, Some(tpe), None))
            }
            THIS => {
                let tpe = self.ty(id);
                let tpe = self.resolved(tpe);
                self.this_tree(tpe)
            }
            TERMREF if self.signature(id).is_some() => {
                let prefix = self.ty(self.kids(id)[0]);
                let prefix = self.resolved(prefix);
                let name = self.name(id);
                let found = match *self.u.types().get(prefix) {
                    Type::Package(p) => self.u.decl(p, &name).unwrap_or_default(),
                    _ => self.u.member_of_type(prefix, &name),
                };
                let sym = self.at_signature(found, id, false)?;
                let tpe = crate::defn::ref_type(self.u, Some(prefix), sym);
                self.ref_tree(tpe)
            }
            TERMREFdirect | TERMREFsymbol | TERMREFpkg | TERMREF | TERMREFin => {
                let tpe = self.ty(id);
                let tpe = self.resolved(tpe);
                self.ref_tree(tpe)
            }
            CLASSconst => {
                let tpe = self.ty(self.kids(id)[0]);
                let class = self.class_symbol("java.lang.Class")?;
                let class_tpe = crate::defn::applied(self.u, class, vec![tpe]);
                Ok(self.bare(T::ClassLiteral { tpe }, Some(class_tpe), None))
            }
            _ => Err("a tree of another kind in a library inline body"),
        }
    }

    /// The constant the tree at `id` is, if it is one.
    fn constant(&self, id: NodeId) -> Option<Constant> {
        let (ast, names) = self.u.file(self.file);
        let node = ast.node(id);
        Some(match tasty::constant(node.tag, node.a)? {
            Const::Unit => Constant::Unit,
            Const::Bool(b) => Constant::Boolean(b),
            Const::Null => Constant::Null,
            Const::Byte(b) => Constant::Byte(b),
            Const::Short(s) => Constant::Short(s),
            Const::Char(_) => Constant::Char(node.a as u16),
            Const::Int(i) => Constant::Int(i),
            Const::Long(l) => Constant::Long(l),
            Const::Float(f) => Constant::Float(f),
            Const::Double(d) => Constant::Double(d),
            Const::Str(r) => Constant::String(names.get(r.0 as usize)?.encode_utf16().collect()),
        })
    }

    /// `this` of type `tpe`, its qualifier empty.
    fn this_tree(&mut self, tpe: TypeId) -> Result<TNodeId, Refusal> {
        let Type::This(c) = *self.u.types().get(tpe) else {
            return Err("this of another kind in a library inline body");
        };
        let cls = self.u.class_symbol(c);
        let empty = TName { name: self.names.intern(""), is_type: true };
        let qual = self.bare(T::Ident { name: empty }, None, None);
        Ok(self.bare(T::This { qual }, Some(tpe), cls))
    }

    /// A reference to path `tpe`: the `this` of an object the body is in, an identifier if its
    /// prefix can be elided, else a selection from its prefix.
    fn ref_tree(&mut self, tpe: TypeId) -> Result<TNodeId, Refusal> {
        let (prefix, s) = match *self.u.types().get(tpe) {
            Type::Package(p) => (None, p),
            Type::Sym(p, s) => (p, s),
            _ => return Err("a path of another kind in a library inline body"),
        };
        if self.u.symbol(s).flags.has(OBJECT)
            && let Some(class) = self.u.module_class(s)
            && self.is_inside(class)
        {
            let class = type_ref(self.u, class);
            let this = self.u.intern_type(Type::This(class));
            return self.this_tree(this);
        }
        let name = TName { name: self.names.intern(&self.u.symbol(s).name.clone()), is_type: false };
        let Some(p) = prefix.filter(|&p| !self.is_elidable(p, s)) else {
            return Ok(self.bare(T::Ident { name }, Some(tpe), Some(s)));
        };
        let qualifier = match *self.u.types().get(p) {
            Type::This(_) if self.u.symbol(s).kind == Kind::Param => {
                return Err("reference to a class parameter in a library inline body");
            }
            Type::This(_) => self.this_tree(p)?,
            Type::Sym(..) | Type::Package(_) => self.ref_tree(p)?,
            _ => return Err("a path through a prefix of another kind in a library inline body"),
        };
        Ok(self.bare(T::Select { qualifier, name }, Some(tpe), Some(s)))
    }

    /// Whether a reference to `s` through `prefix` is an identifier (dotty's `prefixIsElidable`).
    fn is_elidable(&mut self, prefix: TypeId, s: SymbolId) -> bool {
        let static_owner = |u: &mut tasty::Universe, c: SymbolId| {
            u.symbol(c).kind == Kind::Package || u.is_module_class(c) && u.is_static_symbol(c)
        };
        self.u.symbol(s).flags.has(STATIC)
            || match *self.u.types().get(prefix) {
                Type::NoPrefix | Type::Package(_) => true,
                Type::This(c) => self.u.class_symbol(c).is_some_and(|c| {
                    let accessor =
                        self.u.symbol(s).flags.has(PARAMsetter) && !self.u.symbol(c).flags.has(TRAIT);
                    let enclosing = std::iter::successors(Some(self.owner), |&o| self.u.symbol(o).owner)
                        .find(|&o| self.u.symbol(o).kind == Kind::Class);
                    static_owner(self.u, c) || accessor && enclosing == Some(c)
                }),
                Type::Sym(_, m) => self.u.symbol(m).flags.has(OBJECT) && self.u.is_static_symbol(m),
                _ => false,
            }
    }

    /// Whether the body read is inside class `c`.
    fn is_inside(&self, c: SymbolId) -> bool {
        let mut o = Some(self.meth);
        while let Some(x) = o {
            if x == c {
                return true;
            }
            o = self.u.symbol(x).owner;
        }
        false
    }

    /// Path type `t` with its references by name resolved to symbols, the classes of its `this`
    /// types referred to from where they are visible.
    pub fn resolved(&mut self, t: TypeId) -> TypeId {
        match self.u.types().get(t).clone() {
            Type::TermRef(p, n) => {
                let p = self.resolved(p);
                match self.u.resolve(p, &n, false) {
                    Some(s) if self.u.symbol(s).kind == Kind::Package => self.u.intern_type(Type::Package(s)),
                    Some(s) => self.u.intern_type(Type::Sym(Some(p), s)),
                    None => t,
                }
            }
            Type::Sym(Some(p), s) => {
                let p = self.resolved(p);
                self.u.intern_type(Type::Sym(Some(p), s))
            }
            Type::This(c) => match self.u.class_symbol(c) {
                Some(cls) => {
                    let class = type_ref(self.u, cls);
                    self.u.intern_type(Type::This(class))
                }
                None => t,
            },
            _ => t,
        }
    }

    /// The symbol path type `t` refers to.
    pub fn symbol_of(&mut self, t: TypeId) -> Option<SymbolId> {
        match *self.u.types().get(t) {
            Type::Sym(_, s) | Type::Package(s) => Some(s),
            Type::This(c) => self.u.class_symbol(c),
            _ => None,
        }
    }
}
