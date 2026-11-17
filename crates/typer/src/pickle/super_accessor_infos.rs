// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SuperAccessors.scala
//! The types of super accessors.

use crate::defn::type_ref;
use crate::typer::dependent_params::clause_param;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

/// The info of the super accessor of `member` in `class`, `info` its selection's type with the
/// `this` of `member`'s class left in capture sets replaced by `class`'s and references to
/// `member`'s type and value parameters by references to the type's (dotty's `memberInfo`).
pub fn accessor_info(u: &mut Universe, member: SymbolId, class: SymbolId, info: TypeId) -> TypeId {
    let class_this = type_ref(u, class);
    let class_this = u.intern_type(Type::This(class_this));
    let info = match u.symbol(member).owner {
        Some(owner) => with_this(u, info, owner, class_this),
        None => info,
    };
    let info = with_own_value_params(u, member, info);
    let Some((binder, params)) = u.poly(info).map(|b| (b.binder, b.params.to_vec())) else { return info };
    u.map_type(info, &mut |u, x| {
        let (None, s) = u.sym_ref(x)? else { return None };
        let sym = u.symbol(s);
        let ours = sym.kind == Kind::TypeParam && sym.owner == Some(member);
        let i = params.iter().position(|&(n, _)| ours && u.name(n) == sym.name)?;
        Some(u.intern_type(Type::Param(binder, i as u32)))
    })
}

/// Method type `info` with references to the value parameters of `member` replaced by references
/// to the parameters of its method types, in capture sets too: `suffix` of `CC[B]^{this, suffix}`.
fn with_own_value_params(u: &mut Universe, member: SymbolId, info: TypeId) -> TypeId {
    let mut clauses = Vec::new();
    let mut t = info;
    loop {
        if let Some(b) = u.poly(t) {
            t = b.result;
        } else if let Some(m) = u.method(t) {
            clauses.push((m.binder, m.names()));
            t = m.result;
        } else {
            break;
        }
    }
    u.map_with_capture_sets(info, &mut |u, x| {
        let (binder, i) = clauses
            .iter()
            .find_map(|(b, names)| clause_param(u, x, (*b, names), Some(member)).map(|i| (*b, i as u32)))?;
        Some(u.intern_type(Type::Param(binder, i)))
    })
}

/// Type `t` with the `this` of class `owner` replaced by `this_`, in annotations too.
fn with_this(u: &mut Universe, t: TypeId, owner: SymbolId, this_: TypeId) -> TypeId {
    u.map_type(t, &mut |u, x| match *u.ty(x) {
        Type::This(c) => (u.class_symbol(c) == Some(owner)).then_some(this_),
        Type::Annotated(parent, annot) => {
            let (parent, annot) = (with_this(u, parent, owner, this_), with_this(u, annot, owner, this_));
            let annotated = u.intern_type(Type::Annotated(parent, annot));
            Some(u.carry_annotation_tree(x, annotated))
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::accessor_info;
    use tasty::{ANY, Binder, FileId, Flags, Kind, NOTHING, Symbol, SymbolId, Type, Universe};

    fn define(u: &mut Universe, name: &str, kind: Kind, owner: SymbolId) -> SymbolId {
        let flags = Flags::default();
        let (target, tree, decls) = (None, None, Vec::new());
        u.define(Symbol {
            name: name.into(),
            kind,
            owner: Some(owner),
            flags,
            target,
            is_extern: false,
            derived: false,
            decls,
            tree,
        })
    }

    #[test]
    fn accessor_infos_refer_to_their_own_type_parameters() {
        let mut u = Universe::empty();
        let root = u.root();
        let c = define(&mut u, "C", Kind::Class, root);
        let m = define(&mut u, "m", Kind::Def, c);
        let (t, other) = (define(&mut u, "T", Kind::TypeParam, m), define(&mut u, "T", Kind::TypeParam, c));
        let [t, other] = [t, other].map(|s| u.intern_type(Type::Sym(None, s)));
        let (types, terms) = (Binder(FileId(u32::MAX), 0), Binder(FileId(u32::MAX), 1));
        let bounds = u.intern_type(Type::Bounds(NOTHING, ANY));
        let (t_name, x_name) = (u.intern_name("T"), u.intern_name("x"));
        let poly = |u: &mut Universe, param, result| {
            let method = u.intern_method(terms, &[(x_name, param)], result, false, false);
            u.intern_poly(types, &[(t_name, bounds)], method)
        };
        let own = u.intern_type(Type::Param(types, 0));
        let selected = poly(&mut u, t, other);
        let expected = poly(&mut u, own, other);
        assert_eq!(accessor_info(&mut u, m, c, selected), expected);
        let plain = u.intern_method(terms, &[(x_name, t)], t, false, false);
        assert_eq!(accessor_info(&mut u, m, c, plain), plain);
    }
}
