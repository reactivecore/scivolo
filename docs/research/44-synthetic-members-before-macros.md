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
`synthetic::add_synthetic_members` per class, inner first). So our Inlining phase already comes
after typing, as dotty's does; what it lacks is PostTyper's SyntheticMembers *before* it. A macro
therefore sees the typer's class (desugar's `copy`, `apply`, `unapply`, companion `toString`,
`Product`/`Serializable` parents) without `hashCode` … `productElementName`, `MirroredMonoType`,
`fromProduct`/`ordinal` and the `Mirror.*` parent. `Symbol.declarations` of a case class of the
same unit (not only of a macro argument) misses them too; a class of an earlier group comes from
TASTy with them. `L/phases/synthetic.rs` (96 lines) and its parts (`case_members.rs`,
`case_equality.rs`, `case_hash.rs`, `mirrors.rs`, `anon_mirrors.rs`, `sum_mirrors.rs`,
`enum_values.rs`, `value_members.rs`, `mirror_prefixes.rs`, `methods.rs`; 1232 lines) port
SyntheticMembers already and work on the typer's `Tpd` trees and `Universe` symbols.

## 2 What dotty has at macro time [V: scala-cli 3.9.0, `tmp/probe44/`]

Probe: `printTree` (as in the test) of `case class Foo()`, `Foo(a: Int, b: String)`, `case object`,
`enum`, class + companion, nested, `final case class`, user `toString`, `object extends
Serializable`. The backend does not matter: macros run in dotc. Findings:
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
- Bodies as in the check: `hashCode` = `Literal(70822)` (`"Foo".hashCode`) without accessors,
  else the `acc`/`mix`/`finalizeHash` block; `equals` = `this.eq(x$0.$asInstanceOf$[Object]) ||
  (x$0 match { case x$0: Foo @unchecked => … && x$0.canEqual(this); case _ => false })`, the
  `canEqual` conjunct dropped for a `final` class; `toString` = `_toString(this)`;
  `productElement` throws `new IndexOutOfBoundsException(n)` (`Definitions.scala:693`, the Int
  constructor; dotty's own check says `n.toString` and is stale, conformance §8 — the scoring uses
  `tests/dotty-checks/run-macros/tasty-extractors-2.check`, which has `Ident("n")`); `fromProduct`
  binds `a$4`, `b$2` — fresh names of the unit's per-prefix counters, taken in PostTyper.
- Parents: the companion of a case class gets `Mirror.Singleton` (no fields) or `Mirror.Product`
  as a `TypeTree` (`Inferred()`); an enum companion `Mirror.Sum` with `ordinal(x$0)`; enum values'
  anonymous classes `readResolve, productPrefix, toString, ordinal, hashCode`.

## 3 Who depends on it [V: `tmp/triage-1101/`, `tmp/probe44/deps.py`]

- Fails for this reason: tasty-extractors-2 only (:52; :99 is a separate small item, r5 batch 2).
- Checks naming synthetic members: i7987 (passes: library mirrors), tasty-extractors-2.
- Macros inspecting a case class/enum of the same group (regression set, all pass today):
  expr-mirror-info, i11161, i12188, i19732, i22616c, i5941, i8007, quoted-ToExpr-derivation-macro,
  quoted-liftable-derivation-macro{,-2}. gestalt-type-toolbox-reflect and tasty-simplified fail
  for other reasons. `tests/run`: nothing inspects trees (no macro); no dependents.

## 4 Options

- (a) Delay macro expansion until after a PostTyper step: already the case for the expansion;
  what must move is SyntheticMembers. Reframed as (c').
- (b) Synthesize in the reflect views only (`M/reflect/from_symbol.rs:111`, `symbols.rs`): a second
  port of 1232 lines in `M/`, views and trees disagree (`show`, `asTerm`, pickling of the argument
  into quotes), `declarations` of same-unit classes still wrong. Rejected.
- (c) Run the lowering's case phases on macro arguments only: misses same-unit `declarations`,
  needs per-argument scoping, and double synthesis when the argument is also lowered. Rejected.
- (c') Run `L/phases/synthetic.rs` on the whole unit before the inlining phase, from `Units::
  finish`, through a hook the lowering sets (the typer cannot depend on `lower`; `lower` → `macros`
  → `typer`). dotty's order exactly: PostTyper → Inlining → PostInlining (= the lowering's own run,
  which then only touches classes made while inlining). Correct for arguments and same-unit
  classes; ~150 lines; the risk is the lowering's second pass and the fresh-name order.

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
   methods and `serializable_object_method` already skip existing ones (`existing_def`, `has_write_
   replace`; `Universe::define` enters decls, `crates/tasty/src/symbols/define.rs:15`). Add:
   `mirrors.rs mirror_support` returns `None` when the class derives from `scala.deriving.Mirror`
   (`u.derives_from`); `anon_mirrors.rs` keeps the second name take for an `INLINEPROXY`-owned
   copy (dotty's PostInlining) but synthesizes nothing then; `enum_values.rs:44 read_resolve` only
   without an existing `readResolve`. `synthetic.rs:32` then leaves the template alone.
4. Tests: `crates/lower/tests/oracle.rs` and `just lib-compile javalib` unchanged (local names
   and `fromProduct`'s `a$N` are compared); `just dotty-lower-report tasty-extractors-2 run-macros`
   passes (:99 excepted) and the §3 regression set stays green; `tests/macros` gets a macro that
   prints `declarations` of a same-unit case class and one that returns a `case class` argument
   twice (copied class with its members runs: `hashCode`, `fromProduct`).
5. Acceptance: `just gates`; run-macros 297+ of 321; `lib-compile` counts not worse than 11-01
   (3375 same / 80 var-order / 41 differ).

Follow-ups: `NoInits` on classes (typer flag, cosmetic for `Flags.show`); SuperAccessors before
macros (dotty's `wrapTemplate` precedes `addSyntheticMembers`, ours stays in the lowering);
the `n.toString` of `productElement` if the oracle moves past 3.9.0; tasty-extractors-2:99.
