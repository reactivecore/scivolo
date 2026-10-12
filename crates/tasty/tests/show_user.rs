//! Types and definitions as dotty 3.9's REPL shows them.

use std::path::Path;
use tasty::tags::{
    CASE, CHARconst, DOUBLEconst, FLOATconst, INTconst, LAZY, LONGconst, MUTABLE, OBJECT, TRAIT,
};
use tasty::{ANY, Flags, Kind, NOTHING, Symbol, SymbolId, Type, TypeId, Universe};

fn universe() -> Option<Universe> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let stdlib = root.join("ref/libs/scala-library-3.9.0.jar");
    if !stdlib.exists() {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", stdlib.display());
        return None;
    }
    Some(Universe::open(&[stdlib, root.join("libs/javalib-tasty-0.5.12.jar")]).unwrap())
}

fn class(u: &mut Universe, owner: &str, name: &str) -> SymbolId {
    let p = u.package(owner).unwrap();
    u.decl(p, name).unwrap().into_iter().find(|&c| u.symbol(c).kind == Kind::Class).unwrap()
}

fn defs(u: &mut Universe, pkg: &str, owner: &str, name: &str) -> Vec<String> {
    let c = class(u, pkg, owner);
    u.decl(c, name).unwrap().into_iter().map(|m| u.show_def(m)).collect()
}

fn t(u: &mut Universe, full: &str) -> TypeId {
    u.class_ref(full)
}

fn app(u: &mut Universe, tycon: &str, args: &[TypeId]) -> TypeId {
    let tycon = u.class_ref(tycon);
    u.intern_type(Type::Applied(tycon, args.to_vec()))
}

/// The type alias `name` of `Predef`.
fn predef_alias(u: &mut Universe, name: &str) -> TypeId {
    let predef = class(u, "scala", "Predef$");
    let s = u.decl(predef, name).unwrap().into_iter().find(|&s| u.symbol(s).kind == Kind::Type).unwrap();
    u.intern_type(Type::Sym(None, s))
}

fn define(u: &mut Universe, owner: SymbolId, name: &str, kind: Kind, flags: &[u8]) -> SymbolId {
    let flags = flags.iter().fold(Flags::default(), |f, &t| f.with(t));
    let symbol = Symbol {
        name: name.into(),
        kind,
        owner: Some(owner),
        flags,
        target: None,
        is_extern: false,
        derived: false,
        decls: Vec::new(),
        tree: None,
    };
    u.define(symbol)
}

#[test]
fn library_methods() {
    let Some(mut u) = universe() else { return };
    let u = &mut u;
    assert_eq!(defs(u, "scala.collection.immutable", "List", "map"), ["def map[B](f: A => B): List[B]"]);
    assert_eq!(defs(u, "scala", "Predef$", "println"), ["def println(): Unit", "def println(x: Any): Unit"]);
    assert_eq!(defs(u, "scala", "Option", "getOrElse"), ["def getOrElse[B >: A](default: => B): B"]);
    assert_eq!(defs(u, "scala", "Option", "zip"), ["def zip[A1 >: A, B](that: Option[B]): Option[(A1, B)]"]);
    assert_eq!(defs(u, "scala", "Predef$", "summon"), ["def summon[T](using x: T): x.type"]);
    assert_eq!(defs(u, "scala", "Predef$", "???"), ["def ???: Nothing"]);
    assert_eq!(defs(u, "scala", "Predef$", "$conforms"), ["def $conforms[A]: A => A"]);
    assert_eq!(
        defs(u, "scala.collection", "IterableOnceOps", "foldLeft"),
        ["def foldLeft[B](z: B)(op: (B, A) => B): B"]
    );
    assert_eq!(
        defs(u, "scala.collection", "SeqOps", "sorted"),
        ["def sorted[B >: A](implicit ord: Ordering[B]): C"]
    );
    assert_eq!(
        defs(u, "scala.collection", "IterableOps", "partition"),
        ["def partition(p: A => Boolean): (C, C)"]
    );
    assert_eq!(defs(u, "scala.collection", "IterableFactory", "apply"), ["def apply[A](elems: A*): CC[A]"]);
    assert_eq!(
        defs(u, "scala", "Tuple", "map"),
        ["def map[F[_$1]](f: [t] => (x$1: t) => F[t]): Tuple.Map[Tuple.this.type, F]"]
    );
    assert_eq!(
        defs(u, "scala.collection", "MapFactory", "apply"),
        ["def apply[K, V](elems: (K, V)*): CC[K, V]"]
    );
}

#[test]
fn library_values_and_types() {
    let Some(mut u) = universe() else { return };
    let u = &mut u;
    assert_eq!(defs(u, "scala", "Predef$", "String"), ["type String = String"]);
    assert_eq!(
        defs(u, "scala", "Predef$", "Map"),
        ["type Map[K,+V] = Map[K, V]", "val Map: collection.immutable.Map.type"]
    );
    assert_eq!(defs(u, "scala", "package$", "Nil"), ["val Nil: collection.immutable.Nil.type"]);
    assert_eq!(defs(u, "scala", "Predef$", "ArrowAssoc")[..2], ["class ArrowAssoc", "object ArrowAssoc"]);
}

#[test]
fn classes_print_without_their_standard_prefixes() {
    let Some(mut u) = universe() else { return };
    let u = &mut u;
    let int = t(u, "scala.Int");
    let string = t(u, "java.lang.String");
    let cases = [
        (app(u, "scala.collection.immutable.List", &[int]), "List[Int]"),
        (string, "String"),
        (t(u, "java.lang.Object"), "Object"),
        (app(u, "scala.Array", &[int]), "Array[Int]"),
        (app(u, "scala.collection.immutable.Seq", &[int]), "Seq[Int]"),
        (app(u, "scala.collection.Seq", &[int]), "collection.Seq[Int]"),
        (app(u, "scala.collection.mutable.ArrayBuffer", &[int]), "collection.mutable.ArrayBuffer[Int]"),
        (t(u, "scala.collection.immutable.Range.Inclusive"), "collection.immutable.Range.Inclusive"),
        (app(u, "java.util.List", &[int]), "java.util.List[Int]"),
        (t(u, "java.lang.reflect.Method"), "reflect.Method"),
        (t(u, "java.lang.Integer"), "Integer"),
        (app(u, "scala.collection.immutable.Map", &[string, int]), "Map[String, Int]"),
        (app(u, "scala.util.Try", &[int]), "util.Try[Int]"),
        (app(u, "scala.util.Either", &[int, string]), "Either[Int, String]"),
        (app(u, "scala.Option", &[int]), "Option[Int]"),
    ];
    for (ty, shown) in cases {
        assert_eq!(u.show_user(ty), shown);
    }
    let map = predef_alias(u, "Map");
    let map = u.intern_type(Type::Applied(map, vec![string, int]));
    assert_eq!(u.show_user(map), "Map[String, Int]");
}

#[test]
fn functions_tuples_and_infix_types() {
    let Some(mut u) = universe() else { return };
    let u = &mut u;
    let (int, string, boolean) = (t(u, "scala.Int"), t(u, "java.lang.String"), t(u, "scala.Boolean"));
    let int_to_int = app(u, "scala.Function1", &[int, int]);
    let by_name = u.intern_type(Type::ByName(int));
    let pair = app(u, "scala.Tuple2", &[int, string]);
    let or = u.intern_type(Type::Or(int, string, false));
    let product = t(u, "scala.Product");
    let empty = t(u, "scala.EmptyTuple");
    let tail = app(u, "scala.*:", &[string, empty]);
    let cases = [
        (int_to_int, "Int => Int"),
        (app(u, "scala.Function2", &[int, string, boolean]), "(Int, String) => Boolean"),
        (app(u, "scala.Function0", &[int]), "() => Int"),
        (app(u, "scala.Function1", &[int_to_int, int]), "(Int => Int) => Int"),
        (app(u, "scala.Function1", &[int, int_to_int]), "Int => Int => Int"),
        (app(u, "scala.Function1", &[by_name, int]), "(=> Int) => Int"),
        (app(u, "scala.Function1", &[pair, int]), "((Int, String)) => Int"),
        (app(u, "scala.ContextFunction1", &[int, string]), "(Int) ?=> String"),
        (pair, "(Int, String)"),
        (app(u, "scala.Tuple1", &[int]), "Tuple1[Int]"),
        (app(u, "scala.*:", &[int, tail]), "(Int, String)"),
        (tail, "String *: EmptyTuple"),
        (or, "Int | String"),
        (u.intern_type(Type::And(or, product)), "(Int | String) & Product"),
        (u.intern_type(Type::Or(or, boolean, false)), "Int | String | Boolean"),
        (u.intern_type(Type::Or(boolean, or, false)), "Boolean | (Int | String)"),
        (app(u, "scala.=:=", &[int, int]), "Int =:= Int"),
        (app(u, "scala.Option", &[or]), "Option[Int | String]"),
    ];
    for (ty, shown) in cases {
        assert_eq!(u.show_user(ty), shown);
    }
}

#[test]
fn wildcards_constants_singletons_and_lambdas() {
    let Some(mut u) = universe() else { return };
    let u = &mut u;
    let int = t(u, "scala.Int");
    let any_val = t(u, "scala.AnyVal");
    let upper = u.intern_type(Type::Bounds(NOTHING, any_val));
    let lower = u.intern_type(Type::Bounds(int, ANY));
    let unbounded = u.intern_type(Type::Bounds(NOTHING, ANY));
    let scala = u.package("scala").unwrap();
    let scala = u.intern_type(Type::Package(scala));
    let binder = u.fresh_binder();
    let x = u.intern_type(Type::Param(binder, 0));
    let map = app(u, "scala.collection.immutable.Map", &[x, int]);
    let list = app(u, "scala.collection.immutable.List", &[x]);
    let list_class = t(u, "scala.collection.immutable.List");
    let lambda = |u: &mut Universe, result| {
        u.intern_type(Type::Lambda { binder, params: vec![("X".into(), unbounded)], result })
    };
    let cases = [
        (app(u, "scala.Option", &[upper]), "Option[? <: AnyVal]"),
        (app(u, "scala.Option", &[lower]), "Option[? >: Int]"),
        (app(u, "scala.Option", &[unbounded]), "Option[?]"),
        (u.intern_type(Type::Constant(INTconst, 1)), "1"),
        (u.intern_type(Type::Constant(LONGconst, -1)), "-1"),
        (u.intern_type(Type::Constant(CHARconst, 'c' as i64)), "c"),
        (u.intern_type(Type::Constant(DOUBLEconst, 1e10f64.to_bits() as i64)), "1.0E10"),
        (u.intern_type(Type::Constant(FLOATconst, 1.5f32.to_bits() as i64)), "1.5"),
        (u.intern_type(Type::Str("a".into())), "\"a\""),
        (u.intern_type(Type::TermRef(scala, "None".into())), "None.type"),
        (lambda(u, map), "[X] =>> Map[X, Int]"),
        (lambda(u, list), "List"),
        (list_class, "List"),
    ];
    for (ty, shown) in cases {
        assert_eq!(u.show_user(ty), shown);
    }
}

#[test]
fn definitions_of_a_repl_line() {
    let Some(mut u) = universe() else { return };
    let u = &mut u;
    let (int, root) = (t(u, "scala.Int"), u.root());
    let wrapper = define(u, root, "rs$line$1$", Kind::Class, &[OBJECT]);
    let lazy_val = define(u, wrapper, "x", Kind::Val, &[LAZY]);
    let var = define(u, wrapper, "y", Kind::Val, &[MUTABLE]);
    let case_class = define(u, wrapper, "Point", Kind::Class, &[CASE]);
    let point = u.intern_type(Type::Sym(None, case_class));
    let alias = define(u, wrapper, "P", Kind::Type, &[]);
    let object = define(u, wrapper, "O$", Kind::Class, &[OBJECT]);
    let trait_ = define(u, wrapper, "T", Kind::Class, &[TRAIT]);
    let def = define(u, wrapper, "f", Kind::Def, &[]);
    let binder = u.fresh_binder();
    let c = u.intern_type(Type::Sym(None, trait_));
    let using = u.intern_type(Type::Method {
        binder,
        params: vec![("c".into(), c)],
        result: int,
        implicit: true,
        contextual: true,
    });
    let binder = u.fresh_binder();
    let method = u.intern_type(Type::Method {
        binder,
        params: vec![("x".into(), point)],
        result: using,
        implicit: false,
        contextual: false,
    });
    for (s, info) in [(lazy_val, int), (var, point), (alias, point), (def, method)] {
        u.set_info(s, info);
    }
    let shown: Vec<String> =
        [lazy_val, var, case_class, alias, object, trait_, def].map(|s| u.show_def(s)).into();
    let expected = [
        "lazy val x: Int",
        "var y: Point",
        "case class Point",
        "type P = Point",
        "object O",
        "trait T",
        "def f(x: Point)(using c: T): Int",
    ];
    assert_eq!(shown, expected);
    let x = u.intern_type(Type::Sym(None, lazy_val));
    assert_eq!(u.show_user(x), "x.type");
}
