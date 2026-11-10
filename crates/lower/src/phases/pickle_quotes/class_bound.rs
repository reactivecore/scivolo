// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala

use tasty::tags::PRIVATE;
use tasty::{ANY, Kind, SymbolId, Type, TypeId, Universe};

/// The intersection of class `c`'s parents, refined, with its type parameters replaced by
/// `args`, closed over its references to `c` (dotty's `classBound`, applied).
pub fn class_bound(u: &mut Universe, c: SymbolId, args: &[TypeId]) -> TypeId {
    let parents = u.parents(c);
    let parent = parents.into_iter().reduce(|a, b| u.intern_type(Type::And(a, b))).unwrap_or(ANY);
    let raw = refined(u, c, parent);
    let rec = u.fresh_binder();
    let rec_this = u.intern_type(Type::RecThis(rec));
    let mut closed = false;
    let bound = u.map_type(raw, &mut |u, x| {
        if let Some((owner, i)) = u.class_param(x)
            && owner == c
        {
            return args.get(i).copied();
        }
        let s = match *u.ty(x) {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(..) => u.resolve_ref(x),
            Type::This(t) => u.referenced(t),
            _ => None,
        };
        closed |= s == Some(c);
        (s == Some(c)).then_some(rec_this)
    });
    if closed { u.intern_type(Type::Rec(rec, bound)) } else { bound }
}

/// `parent` refined by each declaration of class `c` that narrows the type of a member it
/// inherits, or that is new where `c` derives from `Selectable`.
fn refined(u: &mut Universe, c: SymbolId, parent: TypeId) -> TypeId {
    let site = u.this_type(c);
    let bases: Vec<SymbolId> = u.linearization(c).into_iter().filter(|&b| b != c).collect();
    let selectable = u.class_ref("scala.Selectable");
    let selectable = u.class_symbol(selectable).is_some_and(|s| u.derives_from(c, s));
    let mut refined = parent;
    for d in u.symbol(c).decls.clone() {
        let sym = u.symbol(d);
        if sym.flags.has(PRIVATE) || matches!(sym.kind, Kind::Class | Kind::TypeParam) || u.is_constructor(d)
        {
            continue;
        }
        let name = sym.name.clone();
        let mut inherited = None;
        for &b in &bases {
            for m in u.decls_named(b, &name) {
                if inherited.is_none() && !u.symbol(m).flags.has(PRIVATE) && u.matches(site, d, m) {
                    inherited = Some(m);
                }
            }
        }
        let info = u.info(d);
        let info = match *u.ty(info) {
            Type::Method { .. } | Type::Poly { .. } | Type::ByName(_) => info,
            _ if u.symbol(d).kind == Kind::Def => u.intern_type(Type::ByName(info)),
            _ => info,
        };
        let narrows = match inherited {
            Some(m) => {
                let other = u.member_info(site, m);
                let (a, b) = (expr_widened(u, info), expr_widened(u, other));
                u.is_subtype(a, b) && !u.is_subtype(b, a)
            }
            None => selectable,
        };
        if narrows {
            let name = u.intern_name(&name);
            refined = u.intern_type(Type::Refined(refined, name, info));
        }
    }
    refined
}

fn expr_widened(u: &Universe, t: TypeId) -> TypeId {
    u.by_name(t).unwrap_or(t)
}
