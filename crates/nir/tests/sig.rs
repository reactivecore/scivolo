use nir::demangle;
use nir::sig::{Scope, Sig};
use nir::{Pool, Type, TypeId};

fn ty(pool: &mut Pool, t: Type) -> TypeId {
    TypeId(pool.types.intern(t))
}

fn class(pool: &mut Pool, name: &str, exact: bool, nullable: bool) -> TypeId {
    let name = pool.top(name);
    ty(pool, Type::Ref { name, exact, nullable })
}

#[test]
fn signatures_mangle_as_scala_native_and_demangle_back() {
    let mut pool = Pool::default();
    let (int, unit, ptr) = (ty(&mut pool, Type::Int), ty(&mut pool, Type::Unit), ty(&mut pool, Type::Ptr));
    let (vararg, long) = (ty(&mut pool, Type::Vararg), ty(&mut pool, Type::Long));
    let string = class(&mut pool, "java.lang.String", false, true);
    let exact = class(&mut pool, "C", true, false);
    let ints = ty(&mut pool, Type::Array { elem: int, nullable: true });
    let raw_ints = ty(&mut pool, Type::Array { elem: int, nullable: false });
    let four_ints = ty(&mut pool, Type::ArrayValue(int, 4));
    let pair = ty(&mut pool, Type::StructValue(vec![int, long]));
    let callback = ty(&mut pool, Type::Function(vec![ptr, vararg], int));
    let owner = pool.top("p.C");
    let private_member = pool.member(owner, &Sig::Field("x".into(), Scope::Public));
    let scala_native = [
        (Sig::Field("x".into(), Scope::Public), "F1xO", "x"),
        (Sig::Ctor(vec![int, string]), "RiL16java.lang.StringE", "<init>(int, java.lang.String)"),
        (Sig::Clinit, "IE", "<clinit>"),
        (Sig::Method("map".into(), vec![ints, unit], Scope::Public), "D3mapLAi_uEO", "map(array[int]): unit"),
        (Sig::Method("1st".into(), vec![exact], Scope::PublicStatic), "D3-1stX1CEo", "static 1st(): C"),
        (
            Sig::Method("f".into(), vec![raw_ints, four_ints, int], Scope::Private(owner)),
            "D1fAi_Ai4_iEPT3p.C",
            "private[p.C] f(array[int], [int x 4]): int",
        ),
        (
            Sig::Method("g".into(), vec![pair, int], Scope::PrivateStatic(private_member)),
            "D1gSijEiEpM3p.CF1xO",
            "private static[p.C::x] g({int, long}): int",
        ),
        (Sig::Extern("printf".into()), "C6printf", "extern printf"),
        (Sig::Generated("ü".into()), "G1ü", "generated ü"),
        (Sig::Proxy("cb".into(), vec![callback]), "P2cbRR_viEE", "proxy cb((ptr, ...) => int)"),
    ];
    for (sig, mangled, readable) in scala_native {
        let id = pool.sig(&sig);
        assert_eq!(pool.str(id), mangled);
        assert_eq!(demangle::sig(mangled), readable, "{mangled}");
    }
    assert_eq!(pool.mangle(owner), "T3p.C");
    assert_eq!(pool.mangle(private_member), "M3p.CF1xO");
    assert_eq!(demangle::sig("D1fiEX"), "D1fiEX");
}
