use std::path::Path;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

fn stdlib() -> Option<Universe> {
    let jar = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs/scala-library-3.9.0.jar");
    if !jar.exists() {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", jar.display());
        return None;
    }
    Some(Universe::open(&[jar]).unwrap())
}

fn class(u: &mut Universe, pkg: &str, name: &str) -> SymbolId {
    let p = u.package(pkg).unwrap();
    u.decl(p, name).unwrap().into_iter().find(|&c| u.symbol(c).kind == Kind::Class).unwrap()
}

fn applied(u: &mut Universe, c: SymbolId, args: &[SymbolId]) -> TypeId {
    let tycon = u.intern_type(Type::Sym(None, c));
    let args: Vec<_> = args.iter().map(|&a| u.intern_type(Type::Sym(None, a))).collect();
    u.intern_applied(tycon, &args)
}

#[test]
fn base_types_pass_arguments_through_parents() {
    let Some(mut u) = stdlib() else { return };
    let (list, int) = (class(&mut u, "scala.collection.immutable", "List"), class(&mut u, "scala", "Int"));
    let list_int = applied(&mut u, list, &[int]);
    let seq = class(&mut u, "scala.collection", "Seq");
    let base = u.base_type(list_int, seq).unwrap();
    assert_eq!(u.show(base), "scala.collection.Seq[Int]");
    let function1 = class(&mut u, "scala", "Function1");
    let base = u.base_type(list_int, function1).unwrap();
    assert_eq!(u.show(base), "scala.Function1[scala.Int, Int]");
    let option = class(&mut u, "scala", "Option");
    assert!(u.base_type(list_int, option).is_none());

    let (map, long) = (class(&mut u, "scala.collection.immutable", "Map"), class(&mut u, "scala", "Long"));
    let map_type = applied(&mut u, map, &[long, int]);
    let iterable = class(&mut u, "scala.collection", "Iterable");
    let base = u.base_type(map_type, iterable).unwrap();
    assert_eq!(u.show(base), "scala.collection.Iterable[scala.Tuple2[Long, Int]]");
}

/// `SortedSet`'s parents give `IterableOps` different results `C`, some `@uncheckedVariance`:
/// intersected in their order as dotty's `baseTypeOf` does, the annotation goes.
#[test]
fn base_types_intersect_what_the_parents_of_a_library_class_give() {
    let Some(mut u) = stdlib() else { return };
    let (set, int) =
        (class(&mut u, "scala.collection.immutable", "SortedSet"), class(&mut u, "scala", "Int"));
    let set_int = applied(&mut u, set, &[int]);
    let ops = class(&mut u, "scala.collection", "IterableOps");
    let base = u.base_type(set_int, ops).unwrap();
    let (_, args) = u.applied(base).unwrap();
    let c = args[2];
    assert!(u.annotated(c).is_none() && u.applied(c).is_some_and(|(_, a)| u.annotated(a[0]).is_none()));
    assert_eq!(u.show(c), "scala.collection.immutable.SortedSet[Int]");
}

/// `Map[math.BigInt, Int] & Map[scala.BigInt, Long]` merges: the invariant keys are the same type
/// through an alias, the first is kept (dotty's `glbArgs`, `singletonInterval`).
#[test]
fn intersections_merge_equivalent_invariant_arguments() {
    let Some(mut u) = stdlib() else { return };
    let map = class(&mut u, "scala.collection.immutable", "Map");
    let big_int = class(&mut u, "scala.math", "BigInt");
    let (int, long) = (class(&mut u, "scala", "Int"), class(&mut u, "scala", "Long"));
    let package = class(&mut u, "scala", "package$");
    let alias = u.decl(package, "BigInt").unwrap()[0];
    let tycon = u.intern_type(Type::Sym(None, map));
    let [big_int, alias, int, long] = [big_int, alias, int, long].map(|s| u.intern_type(Type::Sym(None, s)));
    let a = u.intern_applied(tycon, &[big_int, int]);
    let b = u.intern_applied(tycon, &[alias, long]);
    let glb = u.glb(a, b);
    let (_, args) = u.applied(glb).expect("one application of Map");
    assert_eq!(args[0], big_int);
}

/// `Array[1] & Array[Singleton]` and `Array[Int & Singleton] & Array[1]` hold the single type `1`
/// (dotty's `singletonInterval`).
#[test]
fn intersections_merge_a_singleton_with_singleton_bounds() {
    let Some(mut u) = stdlib() else { return };
    let array = class(&mut u, "scala", "Array");
    let (singleton, int) = (u.defn().singleton, class(&mut u, "scala", "Int"));
    let [tycon, singleton, int] = [array, singleton, int].map(|s| u.intern_type(Type::Sym(None, s)));
    let one = u.intern_type(Type::Constant(tasty::tags::INTconst, 1));
    let both = u.intern_type(Type::And(int, singleton));
    let array_one = u.intern_applied(tycon, &[one]);
    let array_singleton = u.intern_applied(tycon, &[singleton]);
    let array_both = u.intern_applied(tycon, &[both]);
    assert_eq!(u.glb(array_one, array_singleton), array_one);
    assert_eq!(u.glb(array_both, array_one), array_one);
}

/// `Option`, its parameter `A` as a type, `Int`, and the wildcard `? <: Int`.
fn option_with_wildcard(u: &mut Universe) -> (SymbolId, TypeId, TypeId, TypeId) {
    let (option, int) = (class(u, "scala", "Option"), class(u, "scala", "Int"));
    let param = u.type_params(option)[0];
    let [param, int] = [param, int].map(|s| u.intern_type(Type::Sym(None, s)));
    let wildcard = u.intern_type(Type::Bounds(tasty::NOTHING, int));
    (option, param, int, wildcard)
}

/// A match type on the wildcard's parameter is its bound; on another scrutinee, its case bodies
/// take the wildcard's upper bound (dotty's `derivedMatchType`).
#[test]
fn approximating_substitution_maps_match_types() {
    let Some(mut u) = stdlib() else { return };
    let (option, a, int, wildcard) = option_with_wildcard(&mut u);
    let case = u.intern_type(Type::MatchCase(int, a));
    let on_param = u.intern_match(tasty::ANY, a, &[case]);
    assert_eq!(u.subst_approx(on_param, option, &[wildcard]), tasty::ANY);

    let on_int = u.intern_match(tasty::ANY, int, &[case]);
    let exact_case = u.intern_type(Type::MatchCase(int, int));
    let expected = u.intern_match(tasty::ANY, int, &[exact_case]);
    assert_eq!(u.subst_approx(on_int, option, &[wildcard]), expected);
}

/// A skolem whose type changes is no longer that value: its new type from above; one whose type
/// stays is kept (dotty's `derivedSkolemType`).
#[test]
fn approximating_substitution_widens_changed_skolems() {
    let Some(mut u) = stdlib() else { return };
    let (option, a, int, wildcard) = option_with_wildcard(&mut u);
    let tycon = u.intern_type(Type::Sym(None, option));
    let option_a = u.intern_applied(tycon, &[a]);
    let changed = u.fresh_skolem(option_a);
    let option_int = u.intern_applied(tycon, &[int]);
    assert_eq!(u.subst_approx(changed, option, &[wildcard]), option_int);

    let kept = u.fresh_skolem(int);
    assert_eq!(u.subst_approx(kept, option, &[wildcard]), kept);
}

/// A `super` whose parent stays open at an invariant place is any type (dotty's
/// `derivedSuperType`): `Array[Super]` becomes `Array[?]`.
#[test]
fn approximating_substitution_gives_up_on_open_supers() {
    let Some(mut u) = stdlib() else { return };
    let (option, a, int, wildcard) = option_with_wildcard(&mut u);
    let array = class(&mut u, "scala", "Array");
    let array = u.intern_type(Type::Sym(None, array));
    let this = this_type(&mut u, option);
    let open = u.intern_type(Type::Super(this, a));
    let array_open = u.intern_applied(array, &[open]);
    let any_bounds = u.intern_type(Type::Bounds(tasty::NOTHING, tasty::ANY));
    let expected = u.intern_applied(array, &[any_bounds]);
    assert_eq!(u.subst_approx(array_open, option, &[wildcard]), expected);

    let exact = u.intern_type(Type::Super(this, int));
    assert_eq!(u.subst_approx(exact, option, &[wildcard]), exact);
}

fn this_type(u: &mut Universe, c: SymbolId) -> TypeId {
    let c = u.intern_type(Type::Sym(None, c));
    u.intern_type(Type::This(c))
}

#[test]
fn overriding_methods_match_and_overloads_do_not() {
    let Some(mut u) = stdlib() else { return };
    let lazy_list = class(&mut u, "scala.collection.immutable", "LazyList");
    let ops = class(&mut u, "scala.collection", "IterableOps");
    let site = this_type(&mut u, lazy_list);
    let (a, b) = (u.decl(lazy_list, "tapEach").unwrap()[0], u.decl(ops, "tapEach").unwrap()[0]);
    assert!(u.matches(site, a, b));

    let hash_map = class(&mut u, "scala.collection.immutable", "HashMap");
    let site = this_type(&mut u, hash_map);
    let own = u.decl(hash_map, "concat").unwrap()[0];
    let iterable_concat = u.decl(ops, "concat").unwrap()[0];
    assert!(!u.matches(site, own, iterable_concat));
    let map_ops = class(&mut u, "scala.collection", "MapOps");
    let map_concat = u.decl(map_ops, "concat").unwrap()[0];
    assert!(u.matches(site, own, map_concat));
}

#[test]
fn bridges_carry_the_overridden_signature() {
    let Some(mut u) = stdlib() else { return };
    let hash_map = class(&mut u, "scala.collection.immutable", "HashMap");
    let concat = u.decl(hash_map, "concat").unwrap()[0];
    let mut bridges = u.bridge_signatures(concat);
    bridges.sort();
    assert_eq!(
        bridges,
        [
            "D6concatL29scala.collection.IterableOnceL28scala.collection.IterableOpsEO",
            "D6concatL29scala.collection.IterableOnceL33scala.collection.immutable.MapOpsEO",
        ]
    );
}

#[test]
fn member_types_as_seen_from_a_prefix() {
    let Some(mut u) = stdlib() else { return };
    let (list, int) = (class(&mut u, "scala.collection.immutable", "List"), class(&mut u, "scala", "Int"));
    let list_int = applied(&mut u, list, &[int]);
    let head = u.member(list, "head")[0];
    let t = u.member_info(list_int, head);
    assert_eq!(u.show(t), "Int");
    let prepended = u.member(list, "::")[0];
    let t = u.member_info(list_int, prepended);
    assert!(u.show(t).contains("scala.collection.immutable.List[B]"), "{}", u.show(t));
}
