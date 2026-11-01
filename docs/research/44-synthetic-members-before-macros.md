# Research 44 — Synthetic members before macros (SyntheticMembers as PostTyper runs it)

`D/` = `ref/scala3/compiler/src/dotty/tools/dotc/`, `Y/` = `crates/typer/src/`, `L/` =
`crates/lower/src/`, `M/` = `crates/macros/src/`. Follows research 01 §3.4, 12 §2a, 35; ADR 0010.
Gap: run-macros `tasty-extractors-2.check:52` (`printTree { case class Foo() }`), triage 10-31 r5
and 11-01 "Not scheduled". Decision: ADR 0071.

## 1 Problem

dotty's phases: Typer → PostTyper (runs `SyntheticMembers.addSyntheticMembers` on every template,
`D/transform/PostTyper.scala:644`) → Pickler → Inlining (`D/transform/Inlining.scala:154`,
`Inlines.needsInlining`: every non-transparent inline call, macros included) → PostInlining
(`addMirrorSupport` for anonymous mirrors made while inlining, `D/transform/PostInlining.scala:30`).
Ours: `Typer` → `Units::finish` (`Y/lib.rs:142-180`: annotations, `zonk_trees`,
`renumber_uniques`, then `with_inlining(false, |i| i.run(typed))` = dotty's Inlining, `Y/inlining/
mod.rs:175`) → `lower_sources` (`L/lib.rs:156`) → `phases::run` (`L/phases/pipeline.rs:25`:
`synthetic::add_synthetic_members` per class, inner first). Our Inlining phase thus already comes
after typing; it lacks PostTyper's SyntheticMembers *before* it. A macro sees the typer's class
(desugar's `copy`, `apply`, `unapply`, companion `toString`, `Product`/`Serializable` parents)
without `hashCode` … `productElementName`, `MirroredMonoType`, `fromProduct`/`ordinal` and the
`Mirror.*` parent; `Symbol.declarations` of any case class of the unit misses them too (a class of
an earlier group comes from TASTy with them). `L/phases/synthetic.rs` and its parts
(`case_members`, `case_equality`, `case_hash`, `mirrors`, `anon_mirrors`, `sum_mirrors`,
`enum_values`, `value_members`, `mirror_prefixes`, `methods`; 1232 lines) port SyntheticMembers
already and work on the typer's `Tpd` trees and `Universe` symbols.

## 2 What dotty has at macro time [V: scala-cli 3.9.0, `tmp/probe44/`]

Probe: the test's `printTree` of `case class Foo()`, `Foo(a: Int, b: String)`, `case object`, `enum`,
class + companion, nested, `final case class`, user `toString` (the backend does not matter):
- Order: synthetic members *before* the typer's body (`syntheticMembers ::: impl.body`,
  `D/transform/SyntheticMembers.scala:729`): `hashCode, equals, toString, canEqual, productArity,
  productPrefix, productElement, productElementName`, then `a, b, copy, copy$default$N, _1, _2`.
  Companion: `apply, unapply, toString` (desugar), then `MirroredMonoType`, `fromProduct` (mirror
  support appends). A case object has no `equals`. `ours: L/phases/synthetic.rs:35` prepends too.
- Flags: `Method | Override | Synthetic` on the members, `Synthetic` on `MirroredMonoType`,
  `Method | Synthetic` on `fromProduct`/`ordinal`; the class gains `NoInits` (typer, not ours yet).
- Positions: every synthetic definition is a point span at the class *name* (`ctx.owner.span.
  focus`, 5:27 for `case class Foo()` at 5:16); desugar's `copy`/`apply` sit at 5:16. Ours:
  `focus = span.point()` of the class def, `L/phases/case_members.rs:33`.
- Bodies as in the check: `hashCode` = `Literal(70822)` (`"Foo".hashCode`) without accessors, else
  the `acc`/`mix`/`finalizeHash` block; `equals` = `this.eq(x$0.$asInstanceOf$[Object]) || (x$0
  match { case x$0: Foo @unchecked => … && x$0.canEqual(this); case _ => false })`, no `canEqual`
  for a `final` class; `toString` = `_toString(this)`; `productElement` throws `new
  IndexOutOfBoundsException(n)` (the Int constructor; dotty's own check says `n.toString` and is
  stale, conformance §8: scored against `tests/dotty-checks/run-macros`, which has `Ident("n")`);
  `fromProduct` binds `a$4`, `b$2`: the unit's per-prefix fresh counters, taken in PostTyper.
- Parents: a case class companion gets `Mirror.Singleton` (no fields) or `Mirror.Product` as a
  `TypeTree` (`Inferred()`), an enum companion `Mirror.Sum` with `ordinal(x$0)`; enum values'
  anonymous classes `readResolve, productPrefix, toString, ordinal, hashCode`.

## 3 Who depends on it [V: `tmp/triage-1101/`, `tmp/probe44/deps.py`]

- Fails for this reason: tasty-extractors-2 only (:52; :99 is a separate small item, r5 batch 2).
  i7987 names `fromProduct` and passes (library mirrors). `tests/run` has no dependents.
- Regression set, macros inspecting a case class or enum of their own group, all passing today:
  expr-mirror-info, i11161, i12188, i19732, i22616c, i5941, i8007, quoted-ToExpr-derivation-macro,
  quoted-liftable-derivation-macro{,-2} (gestalt-type-toolbox-reflect, tasty-simplified: other causes).

## 4 Options

- (a) Delay macro expansion until after a PostTyper step: the expansion is already delayed; what
  must move is SyntheticMembers. Reframed as (c').
- (b) Synthesize in the reflect views only (`M/reflect/from_symbol.rs:111`, `symbols.rs`): a second
  port of 1232 lines, views and trees disagree (`show`, `asTerm`, the argument pickled into a
  quote), same-unit `declarations` still wrong. Rejected.
- (c) The lowering's case phases on macro arguments only: misses same-unit `declarations`, needs
  per-argument scoping, synthesizes twice when the argument is lowered as well. Rejected.
- (c') `L/phases/synthetic.rs` on the whole unit before the inlining phase, from `Units::finish`,
  through a hook the lowering sets (`lower` → `macros` → `typer`, never back). dotty's order:
  PostTyper → Inlining → PostInlining (= the lowering's own run, then touching only classes made
  while inlining). ~150 lines; the risk is the second pass and the fresh-name order.

## 5 Recommendation and plan (one Opus session, S–M)

1. `Y/lib.rs`: `Units` gets `pub synthetic_members: Option<fn(Lent<'_>, TNodeId) -> Result<(),
   (Span, String)>>` (`Lent` from `Y/inlining/host.rs:17`); `finish` calls it after
   `renumber_uniques()` and before `with_inlining(false, …)`, under the same `!typer_only &&
   no errors` guard (typer-compare and the typer's tests stay dotty-typer trees), an `Err` →
   `unsupported_at`.
2. `L/lib.rs`: `Lower::new(u, tpd, names, fresh, sources) -> Result<Lower>` (the struct literal of
   `lower_sources`, which uses it); `pub fn synthetic_members(lent, root)`: `Lower::new(…,
   Sources::new([])?)`, then `for td in class_defs_inner_first(tpd, root) {
   synthetic::add_synthetic_members(l, root, td)? }`. `L/units.rs:52` and `L/compile.rs:61` set
   `units.synthetic_members = Some(synthetic_members)`.
3. Idempotent second pass (`L/phases/`): `case_members`, `value_members`, `enum_value_members`'
   methods and `serializable_object_method` already skip existing ones (`existing_def`;
   `Universe::define` enters decls, `crates/tasty/src/symbols/define.rs:15`). Add: `mirrors.rs
   mirror_support` → `None` when the class derives from `scala.deriving.Mirror`; `anon_mirrors.rs`
   keeps the second name take for an `INLINEPROXY`-owned copy (dotty's PostInlining) but makes
   nothing then; `enum_values.rs:44 read_resolve` only without an existing `readResolve`.
4. Tests: `crates/lower/tests/oracle.rs` and `just lib-compile javalib` unchanged (local names
   and `fromProduct`'s `a$N` are compared); `just dotty-lower-report tasty-extractors-2 run-macros`
   passes (:99 excepted) and the §3 regression set stays green; `tests/macros` gets a macro that
   prints `declarations` of a same-unit case class and one that returns a `case class` argument
   twice (copied class with its members runs: `hashCode`, `fromProduct`).
5. Acceptance: `just gates`; run-macros 297+ of 321; `lib-compile` counts not worse than 11-01
   (3375 same / 80 var-order / 41 differ).

Follow-ups: `NoInits` on classes (typer flag, `Flags.show`); SuperAccessors before macros (dotty's
`wrapTemplate` precedes `addSyntheticMembers`, ours stays in the lowering); tasty-extractors-2:99.
