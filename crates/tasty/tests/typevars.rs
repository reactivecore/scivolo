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
    let union = u.intern_type(Type::Or(option_bool, option_int, false));
    let mut vars = Bounds::default();
    vars.added.push((x, int, false));
    assert!(u.is_subtype_in(option_x, union, &mut vars));
    assert_eq!(vars.added, [(x, int, false), (x, int, true)]);

    assert!(!u.is_subtype(tx, int));
    assert!(u.is_subtype(tx, tx));
    assert!(u.is_subtype(NOTHING, tx));
    assert!(u.is_subtype(tx, ANY));
}

#[test]
fn variables_have_the_atoms_of_their_upper_bound() {
    let Some(mut u) = stdlib() else { return };
    let int = |u: &mut Universe, n| u.intern_type(Type::Constant(tasty::tags::INTconst, n));
    let (one, two) = (int(&mut u, 1), int(&mut u, 2));
    let x = VarId(0);
    let tx = u.intern_type(Type::Var(x));
    let (x_or_1, one_or_2) =
        (u.intern_type(Type::Or(tx, one, false)), u.intern_type(Type::Or(one, two, false)));
    let mut vars = Bounds::default();
    vars.added.push((x, two, true));
    assert!(u.is_subtype_in(x_or_1, one_or_2, &mut vars));
    assert_eq!(vars.added, [(x, two, true)]);
}

#[test]
fn constructor_variables_take_constructors() {
    let Some(mut u) = stdlib() else { return };
    let (int, boolean) = (t(&mut u, "scala.Int", &[]), t(&mut u, "scala.Boolean", &[]));
    let f = VarId(0);
    let tf = u.intern_type(Type::Var(f));
    let f_int = u.intern_type(Type::Applied(tf, vec![int]));
    let f_bool = u.intern_type(Type::Applied(tf, vec![boolean]));

    let list = t(&mut u, "scala.collection.immutable.List", &[]);
    let list_int = t(&mut u, "scala.collection.immutable.List", &[int]);
    let mut vars = Bounds::default();
    assert!(u.is_subtype_in(list_int, f_int, &mut vars));
    assert_eq!(vars.added, [(f, list, false)]);

    let map_int_bool = t(&mut u, "scala.collection.immutable.Map", &[int, boolean]);
    let mut vars = Bounds::default();
    assert!(u.is_subtype_in(map_int_bool, f_bool, &mut vars));
    let [(_, lambda, false)] = vars.added[..] else { panic!("{:?}", vars.added) };
    assert_eq!(u.show(lambda), "[X0: >: Nothing <: Any] =>> Map[Int, <param 0>]");

    let seq = t(&mut u, "scala.collection.Seq", &[]);
    let seq_int = t(&mut u, "scala.collection.Seq", &[int]);
    let mut vars = Bounds::default();
    assert!(u.is_subtype_in(f_int, seq_int, &mut vars));
    assert_eq!(vars.added, [(f, seq, true)]);

    let option_int = t(&mut u, "scala.Option", &[int]);
    assert!(!u.is_subtype_in(option_int, f_bool, &mut Bounds::default()));
}

#[test]
fn class_constructors_compare_as_type_lambdas() {
    let Some(mut u) = stdlib() else { return };
    let (list, seq) =
        (t(&mut u, "scala.collection.immutable.List", &[]), t(&mut u, "scala.collection.Seq", &[]));
    let lambda = |u: &mut Universe, tycon: TypeId| {
        let binder = u.fresh_binder();
        let bounds = u.intern_type(Type::Bounds(NOTHING, ANY));
        let x = u.intern_type(Type::Param(binder, 0));
        let result = u.intern_type(Type::Applied(tycon, vec![x]));
        u.intern_type(Type::Lambda { binder, params: vec![("X".into(), bounds)], result })
    };
    let (to_seq, to_list) = (lambda(&mut u, seq), lambda(&mut u, list));
    assert!(u.is_subtype(list, to_seq));
    assert!(!u.is_subtype(seq, to_list));
    assert!(u.is_subtype(to_list, seq));

    let vector = t(&mut u, "scala.collection.immutable.Vector", &[]);
    let joined = u.lub(list, vector);
    assert!(matches!(u.types().get(joined), Type::Lambda { .. }), "{}", u.show(joined));
    assert!(u.is_subtype(list, joined) && u.is_subtype(vector, joined));
    assert_eq!(u.glb(ANY, list), list);
}
