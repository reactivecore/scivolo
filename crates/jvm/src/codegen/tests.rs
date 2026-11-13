//! The generator on a unit typed and lowered for the JVM: common superclasses and method code.

use super::Gen;
use super::body::Body;
use super::descriptors::method_descriptor;
use classfile::Pool;
use lower::{Backend, Lower};
use std::collections::HashMap;
use std::path::Path;
use syntax::config::Config;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::tpd::{TNodeId, Tree};

/// Runs `f` on the generator of unit `source` and its root; none without the library jars.
fn with_gen<R>(source: &str, f: impl FnOnce(&mut Gen, TNodeId) -> R) -> Option<R> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return None;
    }
    let mut u = Universe::open(&jars).expect("the library");
    let generate = |l: &mut Lower, root, _| {
        let (out, lambdas, classes, tasty) = (Vec::new(), Vec::new(), HashMap::new(), HashMap::new());
        let mut g = Gen { l, out, lambdas, classes, tasty, jdk: classfile::Jdk::default() };
        g.collect_classes(root);
        Ok(f(&mut g, root))
    };
    let sources = [SourceFile::new("t.scala", source)];
    let generated =
        lower::compile_units(&mut u, &sources, Config::from_options(&[]), None, (Backend::Jvm, generate));
    Some(generated.unwrap_or_else(|r| panic!("{}", r.what)).0)
}

/// Finds the definition named `name` in the tree of `root`.
fn definition(g: &Gen, root: TNodeId, name: &str) -> TNodeId {
    let mut todo = vec![root];
    while let Some(t) = todo.pop() {
        if let Some(s) =
            g.l.tpd.sym(t).filter(|_| matches!(g.l.tpd.tree(t), Tree::DefDef { .. } | Tree::TypeDef { .. }))
            && g.l.u.symbol(s).name == name
        {
            return t;
        }
        todo.extend(g.l.tpd.children(t));
    }
    panic!("no definition {name}")
}

/// Generates the code of instance method `name` alone, its parameters from slot 1.
fn code(g: &mut Gen, root: TNodeId, name: &str) -> Vec<u8> {
    let dd = definition(g, root, name);
    let Tree::DefDef { paramss, rhs: Some(rhs), .. } = *g.l.tpd.tree(dd) else {
        panic!("{name} has no body")
    };
    let sym = g.l.tpd.sym(dd).expect("a method's symbol");
    let desc = method_descriptor(g.l.u, sym);
    let (descs, result) = classfile::split_method(&desc);
    let params: Vec<TNodeId> =
        g.l.tpd.clause_items(paramss).iter().flat_map(|&c| g.l.tpd.items(c)).copied().collect();
    let mut pool = Pool::default();
    let mut b = Body::new(&mut pool, None, 1);
    b.method = Some(sym);
    b.result = result.to_owned();
    for (p, d) in params.into_iter().zip(descs) {
        b.bind(g.l.tpd.sym(p).expect("a parameter's symbol"), d.to_owned());
    }
    g.method_body(&mut b, rhs, result).unwrap_or_else(|e| panic!("{name}: {}", e.what));
    assert!(b.code.problem().is_none(), "{name}");
    b.code.bytes
}

const LONG_A: &str = "Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const LONG_B: &str = "Bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[test]
fn common_superclasses_as_the_jvm_sees_them() {
    let source = format!(
        "trait Shape\ntrait Round extends Shape\nclass Base\nclass Circle extends Base with Round
         class Square extends Base with Shape
         object {LONG_A} {{ object {LONG_B} {{ class Stem; class Left extends Stem; class Right extends Stem }} }}"
    );
    let compact = format!("{}$$$$9f4e73fea1176721c6b83decc107abd$$$${}", &LONG_A[..58], &LONG_B[60..]);
    with_gen(&source, |g, root| {
        let table = [
            ("Circle", "Square", "Base"),
            ("Circle", "Circle", "Circle"),
            ("Round", "Shape", "Shape"),
            ("Shape", "Round", "Shape"),
            ("Shape", "Circle", "Shape"),
            ("Circle", "Shape", "Shape"),
            ("Round", "Square", "java/lang/Object"),
            ("Square", "Round", "java/lang/Object"),
            ("Circle", "java/lang/String", "java/lang/Object"),
        ];
        for (a, b, lub) in table {
            assert_eq!(g.jvm_lub(a, b).as_deref(), Some(lub), "{a} {b}");
        }
        assert_eq!(g.jvm_lub("Circle", "NoSuchClass"), None);

        let [stem, left, right] = ["Stem", "Left", "Right"].map(|n| {
            let c = g.l.tpd.sym(definition(g, root, n)).expect("a class symbol");
            g.class_name(c)
        });
        assert_eq!(left, format!("{compact}$Left"));
        assert_eq!(g.jvm_lub(&left, &right), Some(stem));
        assert_eq!(
            g.jvm_lub(&left, &format!("{compact}$Stem")).as_deref(),
            Some(&*format!("{compact}$Stem"))
        );
    });
}

#[test]
fn conditions_loops_and_iinc_as_dotty_generates_them() {
    let source = "object Snippets {
        def loop(n: Int): Int = { var i = 0; while i < n do (if i < 5 then i += 1 else i += 2); i }
        def blockCond(x: Int): Int = if { val y = x; y > 0 } then 1 else 2
        def ifCond(a: Boolean, b: Boolean): Int = if (if a then b else false) then 1 else 2
        def isNull(s: String): Int = if s eq null then 1 else 2
        def notNull(s: String): Int = if null != s then 1 else 2
        def and(a: Int, b: Long): Boolean = a < 0 && b >= 0L
        def bump(x: Int): Int = { var y = x; y = y + 300; y = y - 1; y }
        def nan(d: Double): Int = if !(d < 0.0) then 1 else 2
    }";
    let table: [(&str, &[u8]); 8] = [
        (
            "loop",
            &[
                0x03, 0x3d, 0x1c, 0x1b, 0xa2, 0, 20, 0x1c, 0x08, 0xa2, 0, 9, 0x84, 2, 1, 0xa7, 0xff, 0xf3,
                0x84, 2, 2, 0xa7, 0xff, 0xed, 0x1c, 0xac,
            ],
        ),
        ("blockCond", &[0x1b, 0x3d, 0x1c, 0x03, 0xa4, 0, 5, 0x04, 0xac, 0x05, 0xac]),
        (
            "ifCond",
            &[0x1b, 0x99, 0, 10, 0x1c, 0x9a, 0, 10, 0xa7, 0, 9, 0x03, 0x99, 0, 5, 0x04, 0xac, 0x05, 0xac],
        ),
        ("isNull", &[0x2b, 0xc7, 0, 5, 0x04, 0xac, 0x05, 0xac]),
        ("notNull", &[0x2b, 0xc6, 0, 5, 0x04, 0xac, 0x05, 0xac]),
        ("and", &[0x1b, 0x03, 0xa2, 0, 13, 0x20, 0x09, 0x94, 0x9b, 0, 7, 0x04, 0xa7, 0, 4, 0x03, 0xac]),
        ("bump", &[0x1b, 0x3d, 0xc4, 0x84, 0, 2, 0x01, 0x2c, 0x84, 2, 0xff, 0x1c, 0xac]),
        ("nan", &[0x27, 0x0e, 0x98, 0x9b, 0, 5, 0x04, 0xac, 0x05, 0xac]),
    ];
    with_gen(source, |g, root| {
        for (name, bytes) in table {
            assert_eq!(code(g, root, name), bytes, "{name}");
        }
    });
}

#[test]
fn dead_code_after_a_nothing_call_keeps_its_stack() {
    let source = "object Nothings {
        def unimplemented: Int = ???
        def guarded(c: Boolean): Int = if (if c then true else ???) then 1 else 2
        def early(x: Int): Int = { if x > 0 then return 1; ??? }
    }";
    let call = [0xb2, 0, 0, 0xb6, 0, 0, 0xbf];
    let table: [(&str, Vec<u8>); 3] = [
        ("unimplemented", [&call[..], &[0xac]].concat()),
        (
            "guarded",
            [
                &[0x1b, 0x99, 0, 10, 0x04, 0x9a, 0, 16, 0xa7, 0, 15][..],
                &call,
                &[0x99, 0, 5, 0x04, 0xac, 0x05, 0xac],
            ]
            .concat(),
        ),
        ("early", [&[0x1b, 0x03, 0xa4, 0, 5, 0x04, 0xac][..], &call, &[0xac]].concat()),
    ];
    with_gen(source, |g, root| {
        for (name, bytes) in table {
            let mut code = code(g, root, name);
            let at = code.iter().position(|&op| op == 0xb2).expect("a getstatic");
            for i in [at + 1, at + 2, at + 4, at + 5] {
                code[i] = 0;
            }
            assert_eq!(code, bytes, "{name}");
        }
    });
}
