use std::path::Path;
use tasty::{ANY, Kind, Mark, NOTHING, SymbolId, Type, TypeId, TypeVars, Universe, VarId};

fn stdlib() -> Option<Universe> {
    let jar = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs/scala-library-3.9.0.jar");
    if !jar.exists() {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", jar.display());
        return None;
    }
    Some(Universe::open(&[jar]).unwrap())
}

/// A constraint that keeps every bound added, a new bound checked against the others.
#[derive(Default)]
struct Bounds {
    added: Vec<(VarId, TypeId, bool)>,
}

impl Bounds {
    fn of(&self, v: VarId, upper: bool) -> Vec<TypeId> {
        self.added.iter().filter(|&&(w, _, up)| w == v && up == upper).map(|&(_, t, _)| t).collect()
    }

    fn add(&mut self, u: &mut Universe, v: VarId, t: TypeId, upper: bool) -> bool {
        let others = self.of(v, !upper);
        let consistent = others.into_iter().all(|o| match upper {
            true => u.is_subtype(o, t),
            false => u.is_subtype(t, o),
        });
        consistent && {
            self.added.push((v, t, upper));
            true
        }
    }
}

impl TypeVars for Bounds {
    fn instance(&self, _: VarId) -> Option<TypeId> {
        None
    }
    fn add_lower(&mut self, u: &mut Universe, v: VarId, lo: TypeId) -> bool {
        self.add(u, v, lo, false)
    }
    fn add_upper(&mut self, u: &mut Universe, v: VarId, hi: TypeId) -> bool {
        self.add(u, v, hi, true)
    }
    fn bounds(&self, v: VarId) -> (TypeId, TypeId) {
        (
            self.of(v, false).first().copied().unwrap_or(NOTHING),
            self.of(v, true).first().copied().unwrap_or(ANY),
        )
    }
    fn frozen(&self) -> bool {
        false
    }
    fn mark(&self) -> Mark {
        Mark(self.added.len())
    }
    fn rollback(&mut self, m: Mark) {
        self.added.truncate(m.0);
    }
}

fn class(u: &mut Universe, full: &str) -> SymbolId {
    let (pkg, name) = full.rsplit_once('.').unwrap();
    let p = u.package(pkg).unwrap();
    u.decl(p, name).unwrap().into_iter().find(|&c| u.symbol(c).kind == Kind::Class).unwrap()
}

fn t(u: &mut Universe, full: &str, args: &[TypeId]) -> TypeId {
    let c = class(u, full);
    let c = u.intern_type(Type::Sym(None, c));
    match args.is_empty() {
        true => c,
        false => u.intern_type(Type::Applied(c, args.to_vec())),
    }
}

#[test]
fn comparisons_constrain_variables() {
    let Some(mut u) = stdlib() else { return };
    let (int, any) = (t(&mut u, "scala.Int", &[]), ANY);
    let x = VarId(0);
    let tx = u.intern_type(Type::Var(x));

    let list_x = t(&mut u, "scala.collection.immutable.List", &[tx]);
    let seq_int = t(&mut u, "scala.collection.Seq", &[int]);
    let mut vars = Bounds::default();
    assert!(u.is_subtype_in(list_x, seq_int, &mut vars));
    assert_eq!(vars.added, [(x, int, true)]);

    let option_int = t(&mut u, "scala.Option", &[int]);
    let option_x = t(&mut u, "scala.Option", &[tx]);
    let mut vars = Bounds::default();
    assert!(u.is_subtype_in(option_int, option_x, &mut vars));
    assert_eq!(vars.added, [(x, int, false)]);

    let f_x = t(&mut u, "scala.Function1", &[tx, int]);
    let f_int = t(&mut u, "scala.Function1", &[int, any]);
    let mut vars = Bounds::default();
    assert!(u.is_subtype_in(f_x, f_int, &mut vars));
    assert_eq!(vars.added, [(x, int, false)]);

    let array_x = t(&mut u, "scala.Array", &[tx]);
    let array_int = t(&mut u, "scala.Array", &[int]);
    let mut vars = Bounds::default();
    assert!(u.is_same_type_in(array_x, array_int, &mut vars));
    assert_eq!(vars.added, [(x, int, true), (x, int, false)]);
}

#[test]
fn contradictions_fail_and_either_rolls_back() {
    let Some(mut u) = stdlib() else { return };
    let (int, boolean) = (t(&mut u, "scala.Int", &[]), t(&mut u, "scala.Boolean", &[]));
    let x = VarId(0);
    let tx = u.intern_type(Type::Var(x));
    let mut vars = Bounds::default();
    assert!(u.is_subtype_in(tx, int, &mut vars));
    assert!(!u.is_subtype_in(boolean, tx, &mut vars));

    let option_x = t(&mut u, "scala.Option", &[tx]);
    let option_bool = t(&mut u, "scala.Option", &[boolean]);
    let option_int = t(&mut u, "scala.Option", &[int]);
    let union = u.intern_type(Type::Or(option_bool, option_int));
    let mut vars = Bounds::default();
    vars.added.push((x, int, false));
    assert!(u.is_subtype_in(option_x, union, &mut vars));
    assert_eq!(vars.added, [(x, int, false), (x, int, true)]);

    assert!(!u.is_subtype(tx, int));
    assert!(u.is_subtype(tx, tx));
    assert!(u.is_subtype(NOTHING, tx));
    assert!(u.is_subtype(tx, ANY));
}
