//! Erasure (dotty's `TypeErasure`), binary names and Scala Native's method signatures.

mod support;

use support::{parse, universe};
use tasty::tags::{OBJECT, SYNTHETIC};
use tasty::{Binder, Erased, FileId, Flags, Kind, NOTHING, Symbol, SymbolId, Type, TypeId, Universe};

fn class(name: &str) -> Erased {
    Erased::Class(name.into())
}

fn array(elem: Erased) -> Erased {
    Erased::Array(Box::new(elem))
}

#[test]
fn erasure_of_types() {
    let Some(mut u) = universe() else { return };
    let dotty = [
        ("scala.Int", Erased::Prim('i')),
        ("scala.Boolean", Erased::Prim('z')),
        ("scala.Unit", Erased::Unit),
        ("scala.Nothing", Erased::Nothing),
        ("scala.Null", Erased::Null),
        ("scala.Any", class("java.lang.Object")),
        ("1", Erased::Prim('i')),
        ("\"a\"", class("java.lang.String")),
        ("scala.Predef.String", class("java.lang.String")),
        ("scala.Option[scala.Int]", class("scala.Option")),
        ("scala.None.type", class("scala.None$")),
        ("scala.Function1[scala.Int, scala.Int]", class("scala.Function1")),
        ("oracle.Pos", Erased::Prim('i')),
        ("oracle.Pair[scala.Int]", class("scala.Tuple2")),
        ("oracle.Box", class("oracle.Box")),
        ("oracle.a.type", class("oracle.Box")),
        ("oracle.BoxInt", class("oracle.Box")),
    ];
    for (text, expected) in dotty {
        let t = parse(&mut u, text);
        assert_eq!(u.erase(t), expected, "{text}");
    }
}

#[test]
fn erasure_of_arrays() {
    let Some(mut u) = universe() else { return };
    let dotty = [
        ("scala.Array[scala.Int]", array(Erased::Prim('i'))),
        ("scala.Array[scala.Array[java.lang.String]]", array(array(class("java.lang.String")))),
        ("scala.Array[scala.Unit]", array(class("scala.runtime.BoxedUnit"))),
        ("scala.Array[scala.Any]", array(class("java.lang.Object"))),
        ("scala.Array[? <: scala.Int]", array(Erased::Prim('i'))),
        ("scala.Array[? <: java.lang.String]", array(class("java.lang.String"))),
        ("scala.Array[?]", class("java.lang.Object")),
        ("scala.Array[? <: scala.AnyVal]", class("java.lang.Object")),
        ("scala.Array[? <: oracle.Pair[scala.Int]]", array(class("scala.Tuple2"))),
        ("scala.Array[? <: oracle.Pos]", array(Erased::Prim('i'))),
    ];
    for (text, expected) in dotty {
        let t = parse(&mut u, text);
        assert_eq!(u.erase(t), expected, "{text}");
    }
}

#[test]
fn erasure_of_unions_and_intersections() {
    let Some(mut u) = universe() else { return };
    let list_or_vector =
        "scala.collection.immutable.List[scala.Int] | scala.collection.immutable.Vector[scala.Int]";
    let dotty = [
        ("java.lang.String | scala.Null", class("java.lang.String")),
        ("scala.Nothing | scala.Int", Erased::Prim('i')),
        ("scala.Int | java.lang.String", class("java.lang.Object")),
        ("scala.Int | scala.Long", class("java.lang.Object")),
        ("1 | 2", Erased::Prim('i')),
        ("scala.Some[scala.Int] | scala.None.type", class("scala.Option")),
        (list_or_vector, class("scala.collection.generic.DefaultSerializable")),
        ("oracle.Dog | oracle.Animal", class("oracle.Animal")),
        ("scala.Array[scala.Int] | scala.Array[scala.Int]", array(Erased::Prim('i'))),
        ("scala.Array[scala.Int] | scala.Array[scala.Long]", class("java.lang.Object")),
        ("scala.Array[java.lang.String] | scala.Array[oracle.Box]", array(class("java.lang.Object"))),
        ("scala.Array[oracle.Dog] | scala.Array[oracle.Animal]", array(class("oracle.Animal"))),
        ("oracle.Dog & oracle.Animal", class("oracle.Dog")),
        ("oracle.Animal & oracle.Dog", class("oracle.Dog")),
        ("scala.Product & java.io.Serializable", class("java.io.Serializable")),
        ("java.io.Serializable & scala.Product", class("java.io.Serializable")),
        ("java.lang.Object & scala.Array[scala.Int]", array(Erased::Prim('i'))),
        ("oracle.Structural & oracle.Box", class("oracle.Box")),
    ];
    for (text, expected) in dotty {
        let t = parse(&mut u, text);
        assert_eq!(u.erase(t), expected, "{text}");
    }
}

#[test]
fn value_classes_erase_to_their_underlying_value_or_class() {
    let Some(mut u) = universe() else { return };
    let (rich_int, int) = (parse(&mut u, "scala.runtime.RichInt"), parse(&mut u, "scala.Int"));
    assert_eq!(u.erase(rich_int), Erased::Prim('i'));
    assert_eq!(
        u.erase_value(rich_int),
        Erased::Value("scala.runtime.RichInt".into(), Box::new(Erased::Prim('i')))
    );
    assert_eq!(u.erase_boxed(rich_int), class("scala.runtime.RichInt"));
    assert_eq!(u.erase_boxed(int), Erased::Prim('i'));
    assert!(u.is_value_class(rich_int) && !u.is_value_class(int));
}

/// `[A <: hi_a, B <: hi_b](b: B): A`, `hi_b` made from `A`.
fn poly(u: &mut Universe, hi_a: TypeId, hi_b: fn(&mut Universe, TypeId) -> TypeId) -> TypeId {
    let (types, terms) = (Binder(FileId(u32::MAX), 1), Binder(FileId(u32::MAX), 2));
    let (a, b) = (u.intern_type(Type::Param(types, 0)), u.intern_type(Type::Param(types, 1)));
    let bounds_a = u.intern_type(Type::Bounds(NOTHING, hi_a));
    let hi_b = hi_b(u, a);
    let bounds_b = u.intern_type(Type::Bounds(NOTHING, hi_b));
    let params = [(u.intern_name("A"), bounds_a), (u.intern_name("B"), bounds_b)];
    let b_name = u.intern_name("b");
    let method = u.intern_method(terms, &[(b_name, b)], a, false, false);
    u.intern_poly(types, &params, method)
}

fn array_of(u: &mut Universe, elem: TypeId) -> TypeId {
    let array = u.class_ref("scala.Array");
    u.intern_applied(array, &[elem])
}

#[test]
fn type_parameters_erase_to_their_bounds() {
    let Some(mut u) = universe() else { return };
    let [int_array, int, any] =
        ["scala.Array[scala.Int]", "scala.Int", "scala.Any"].map(|t| parse(&mut u, t));
    let (ints, object) = (array(Erased::Prim('i')), class("java.lang.Object"));
    let dotty = [
        (int_array, (|_, a| a) as fn(&mut Universe, TypeId) -> TypeId, ints.clone(), ints.clone()),
        (any, array_of, object.clone(), object),
        (int, array_of, ints, Erased::Prim('i')),
    ];
    for (hi_a, hi_b, param, result) in dotty {
        let t = poly(&mut u, hi_a, hi_b);
        assert_eq!(u.erased_method_type(t), (vec![param], result), "{}", u.show(t));
    }
}

#[test]
fn unit_parameters_are_boxed_and_by_name_ones_functions() {
    let Some(mut u) = universe() else { return };
    let (unit, int) = (parse(&mut u, "scala.Unit"), parse(&mut u, "scala.Int"));
    let by_name = u.intern_type(Type::ByName(int));
    let params = [(u.intern_name("x"), unit), (u.intern_name("f"), by_name)];
    let method = u.intern_method(Binder(FileId(u32::MAX), 3), &params, unit, false, false);
    let expected = vec![class("scala.runtime.BoxedUnit"), class("scala.Function0")];
    assert_eq!(u.erased_method_type(method), (expected, Erased::Unit));
}

/// The member `name` of class `class`, the first if overloaded.
fn member(u: &mut Universe, class: &str, name: &str) -> SymbolId {
    let c = u.class_ref(class);
    let c = u.class_symbol(c).unwrap();
    u.decl(c, name).unwrap()[0]
}

#[test]
fn method_signatures_of_library_methods() {
    let Some(mut u) = universe() else { return };
    let scala_native = [
        ("scala.Option", "isEmpty", "D7isEmptyzEO"),
        ("scala.Option", "getOrElse", "D9getOrElseL15scala.Function0L16java.lang.ObjectEO"),
        ("scala.Option", "contains", "D8containsL16java.lang.ObjectzEO"),
        (
            "scala.collection.immutable.List",
            "::",
            "D12$colon$colonL16java.lang.ObjectL31scala.collection.immutable.ListEO",
        ),
        ("scala.Predef$", "require", "D7requirezuEO"),
        ("scala.runtime.RichInt", "max", "D3maxiiEO"),
    ];
    for (class, name, expected) in scala_native {
        let m = member(&mut u, class, name);
        assert_eq!(u.method_signature(m).as_deref(), Some(expected), "{class}.{name}");
    }
    let any_eq = member(&mut u, "scala.Any", "==");
    assert_eq!(u.method_signature(any_eq), None);
}

fn symbol(u: &mut Universe, name: &str, kind: Kind, owner: SymbolId) -> SymbolId {
    u.define(Symbol {
        name: name.into(),
        kind,
        owner: Some(owner),
        flags: Flags::default(),
        target: None,
        is_extern: false,
        derived: false,
        decls: Vec::new(),
        tree: None,
    })
}

/// Defines `object name` in `owner`: its value and module class, flagged `flags` and `OBJECT`.
fn object(u: &mut Universe, name: &str, owner: SymbolId, flags: Flags) -> (SymbolId, SymbolId) {
    let flags = flags.with(OBJECT);
    let value = symbol(u, name, Kind::Val, owner);
    let class = symbol(u, &format!("{name}$"), Kind::Class, owner);
    u.set_flags(value, flags);
    u.set_flags(class, flags);
    (value, class)
}

#[test]
fn binary_names_of_nested_classes_and_target_names() {
    let mut u = Universe::empty();
    let root = u.root();
    let p = symbol(&mut u, "p", Kind::Package, root);
    let outer = symbol(&mut u, "Outer", Kind::Class, p);
    let inner = symbol(&mut u, "Inner", Kind::Class, outer);
    let deeper = symbol(&mut u, "Deeper", Kind::Class, inner);
    let (_, nested) = object(&mut u, "M", outer, Flags::default());
    let (_, o) = object(&mut u, "O", p, Flags::default());
    let in_object = symbol(&mut u, "C", Kind::Class, o);
    let top = symbol(&mut u, "Top", Kind::Class, root);
    let operator = symbol(&mut u, "::", Kind::Class, p);
    let dotty = [
        (outer, "p.Outer"),
        (inner, "p.Outer$Inner"),
        (deeper, "p.Outer$Inner$Deeper"),
        (nested, "p.Outer$M$"),
        (o, "p.O$"),
        (in_object, "p.O$C"),
        (top, "Top"),
        (operator, "p.$colon$colon"),
    ];
    for (c, expected) in dotty {
        assert_eq!(u.binary_name(c), expected);
    }

    let named = symbol(&mut u, "Named", Kind::Class, p);
    u.set_target(named, "Renamed");
    let (value, obj) = object(&mut u, "Obj", p, Flags::default());
    u.set_target(value, "Tgt");
    let (_, companion) = object(&mut u, "Named", p, Flags::default().with(SYNTHETIC));
    let member = symbol(&mut u, "In", Kind::Class, named);
    assert_eq!(u.class_target(named).as_deref(), Some("Renamed"));
    assert_eq!(u.class_target(obj).as_deref(), Some("Tgt$"));
    assert_eq!(u.class_target(companion).as_deref(), Some("Renamed$"));
    assert_eq!(u.class_target(o), None);
    let dotty = [(named, "p.Renamed"), (obj, "p.Tgt$"), (companion, "p.Renamed$"), (member, "p.Renamed$In")];
    for (c, expected) in dotty {
        assert_eq!(u.binary_name(c), expected);
    }
}

#[test]
fn long_nested_names_are_compactified() {
    let mut u = Universe::empty();
    let root = u.root();
    let p = symbol(&mut u, "p", Kind::Package, root);
    let outer = symbol(&mut u, "Outer", Kind::Class, p);
    let long = "x".repeat(228);
    let inner = symbol(&mut u, &long, Kind::Class, outer);
    assert_eq!(u.binary_name(inner), format!("p.Outer${long}"));
    u.rename(inner, &format!("{long}x"));
    let compact =
        format!("Outer${}$$$$872eb7f121463feb73835a2654de0b8$$$${}", "x".repeat(52), "x".repeat(58));
    assert_eq!(u.binary_name(inner), format!("p.{compact}"));
    assert_eq!(u.expanded_binary_name(inner), format!("p.Outer${long}x"));
}
