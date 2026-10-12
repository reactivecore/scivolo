# 12 — Inlining: the rest of #82

Date: 2026-10-12. Claims are [V] verified by reading the code or running it, or [I] inferred.
Paths are relative to `ref/scala3` (tag 3.9.0; `N/` = `compiler/src/dotty/tools/dotc/inlines/`,
`T/` = `.../dotc/typer/`, `X/` = `.../dotc/transform/`, `C/` = `.../dotc/core/`); our code at
e884b16 (`I/` = `crates/typer/src/inlining/`, `L/` = `crates/lower/src/`). Follows [research 09 §1,
§5 MR 7](09-typer.md) and [10](10-lowering.md); #82 steps 1, 2a and 2b are merged.

**Recommendation in one paragraph.** The payoff is not where the roadmap puts it. Of the 1446
single-file `tests/run` tests that pass in the VM, 314 stop at `transparent inline method call`
and 293 of those only because of `Predef.assert` (`summon` adds 24): a transparent inline method
whose body lives in the library's TASTy [V]. Replacing `assert(` by `require(` and `summon[` by
`implicitly[` in those 314 sources lowers 159 of them [V]. So the order is: wire the existing
`inlining::run` into the lowering (a, ≤13 tests), expand transparent calls in the typer for
same-unit methods (c, ≤16), read library inline bodies from TASTy (d, turns on `assert`/`summon`,
≈150), then `inline if`/`match` (b, ≤9 direct, and the door to e) and `scala.compiletime` (e).
Everything stays a port: `Inliner`/`InlineTyper` (done), `InlineReducer`, `TreeUnpickler.readTerm`,
`Inlines.InlineCall.expand`, `TypeEval`.

## 1. Where we are

Sweep of 2026-10-12 (`tmp/research-12/sweep.py`: `target/release/lower` over the single-file tests
with VM baseline `passed`, no VM run) [V]: 1446 tests, 400 lowered, 986 typer refused, 47 lowering
refused, 13 parser refused (the roadmap's 399/987/46). Inline-related first refusals:

| tests | refusal | where | item |
|---:|---|---|---|
| 314 | transparent inline method call (293 with `assert(`, 24 with `summon[`, 9 with own inline defs) | `crates/typer/src/typer/inline_refs.rs:16` | c + d |
| 17 | deferred, given or overriding inline definition | `namer/checks.rs:110` | retained inline methods, later |
| 13 | transparent inline definition | `namer/checks.rs:106` | c |
| 9 | inline or sub match | `typer/matches.rs:23` | b |
| 8 + 5 | inline methods / a call of an inline method (lowering) | `L/codegen/body.rs:51`, `L/codegen/callee.rs:29` | a |
| 7 | non-public member in an inline method | `inline_refs.rs:28` | inline accessors, later |
| 3 + 1 | scala.compiletime operation, reference to an erased definition | `inline_refs.rs:19,25` | e |

After `assert`/`summon` (`tmp/research-12/sweep2.py`) the 314 become: 159 lowered, 120 typer
refused (23 `no implicit argument`, 13 `transparent inline method call` from other methods, 11
`comparison of a converted value`, …), 34 lowering refused, 1 crash [V]. Feature use among the
1446 tests: 126 define inline methods, 20 `transparent`, 18 `inline match`, 6 `inline if`, 43
import `scala.compiletime`, 554 call `assert`, 79 `summon`, 59 use quotes (out of scope) [V].
`locally`, `require` and `IArray(…)` are not inline calls for our typer: the probes lower today
[V]; the roadmap's "`locally`, `require`" should read "`assert`, `summon`". The `IArray` probe
emits calls to `IArray$package$IArray$.apply` [V], a method dotty erases (`X/Erasure.scala:881`,
`isEffectivelyErased`) and the library NIR therefore lacks [I]: the lowering must refuse or inline
every call of an inline method, never emit it.

What exists [V]: `I/` (1817 lines, 11 files): `run`, `Inlined` nodes (`tpd/tree.rs:249`), the
copier with symbol substitution (`copy.rs`), parameter and `this` proxies (`proxies.rs`,
`this_proxy.rs`), beta reduction (`beta.rs`), the retyper with constant folding and constant `if`s
(`retype.rs`, `constants.rs`), `dropUnusedDefs` (`unused.rs`), the `--inlined` oracle
(`scripts/typer-oracle`, `scripts/typer-compare.py`, `crates/typer/tests/inlined/*.check`, `just
typer-checks`). Only methods with a body in the unit expand (`I/mod.rs:135`); `retype.rs:117`
refuses bodies with `match`, `try`, `return`; the typer refuses inline matches, transparent
definitions and calls, compiletime operations.

## 2. The five items

### a. Lowering `Inlined` trees

- dotty: `Inlining` runs after `pickler` (`Compiler.scala:52`), `X/Inlining.scala:154-162`
  (`needsInlining` → `inlineCall`); PostTyper shrinks `call` to a trace (`X/PostTyper.scala:634`,
  `N/Inlines.scala:343`); the node survives to erasure, where `typedInlined` drops it
  (`X/Erasure.scala:875-878`, `N/Inlines.scala:282`: bindings and expansion as a `Block`, or the
  expansion alone; `reposition` gives trees from another source the call's span, same-source trees
  keep theirs). Inline methods themselves are erased, `isEffectivelyErased` (`X/Erasure.scala:
  881-894`, `C/SymDenotations.scala:1079`); retained ones (overriding, given, deferred) keep a body
  via `bodyRetainer` (`N/Inlines.scala:267`, `X/Erasure.scala:993`). nscplugin never sees `Inlined`
  except `PostInlineNativeInterop.scala:77` (interop, not ours) [V]. In between, phases look
  *through* it like a block: `TreeInfo.scala:95,99,635` (purity), `X/BetaReduce.scala:86`,
  `X/ContextFunctionResults.scala:134` [V].
- ours: nothing calls `inlining::run` outside `typer-dump --inlined` (`crates/typer/src/dump.rs:54`);
  `L/lib.rs:90` `lower` goes straight to `phases::run` (`L/phases/mod.rs:86`, `beta_reduce` first)
  [V].
- gap: call `run` in `lower` before `phases::run`; a pass `drop_inlined` right after it (dotty's
  `dropInlined` pulled forward: every pass in between treats the node as its block, so the NIR is
  the same — ADR 0010's condition; bindings get the call span as `normalizeBinding` does,
  `N/InlineReducer.scala:148`); `erasure/` drops inline `DefDef`s instead of codegen refusing them;
  `callee.rs:29` keeps refusing a call that survived (library inline methods until d). Positions:
  same-source trees keep their spans, so `nir-cmp --debug` must match on the golden inputs [I].
- size ~400–600 Rust [I]; unblocks ≤ 13 tests (8 + 5 above) [V].

### b. `inline if`, `inline match`, constant folding

- dotty: `InlineTyper.typedIf` (`N/Inliner.scala:962-978`: condition under `Mode.ForceInline`,
  non-constant `inline if` is an error), `typedMatch`/`typedMatchFinish` (:1035-1124: flatten
  bindings, `reduceInlineMatch`, `dropUnusedDefs`, retype the case), `InlineReducer.reduceInlineMatch`
  (`N/InlineReducer.scala:172-454`: scrutinee binding `$scrutineeN`, `reducePattern` for `Typed`,
  `Bind`, wildcard, `Literal`, `RefTree`, `UnApply` of case classes with synthetic `unapply`,
  `Alternative`; type bindings through GADT approximation; guards must fold to constants; sub
  matches), `reduceProjection` (:86, `new C(args).x` → arg, needs `isNoInitsRealClass`),
  `normalizeBinding` (:135). The typer only checks the context (`T/Typer.scala:1626,2200`); TASTy
  writes an `INLINE` modifier before `IF`/`MATCH` (`C/tasty/TreeUnpickler.scala:1554`) [V].
- ours: `retype.rs:57-68` folds constant `if`s (`is_idempotent_expr` instead of the error);
  `retype.rs:117` refuses `match`; the typer refuses `MatchKind::Inline | Sub`
  (`typer/matches.rs:23`); tpd `If`/`Match` carry no inline flag (`tpd/tree.rs:135,163`); the
  oracle prints `InlineIf`/`InlineMatch` as class names (`scripts/typer-oracle/Sexp.scala:131`),
  so the flag is observable [V]. `no_inits.rs` and `beta.rs` give `reduceProjection` its parts.
- gap: `inline: bool` on tpd `If` and `Match` plus printer; the typer types them as plain ones
  (`T/Typer.scala:2181-2200` adds nothing else); `I/reduce/` with `reduce_match`, `patterns`,
  `scrutinee`, `projection`; `retype.rs` dispatches `Match` to it and plain `match` bodies to a
  retyped copy (cases retyped like blocks). Refuse: type-variable bindings in patterns (GADT),
  `summonFrom` until e, `erased` scrutinees.
- size ~800–1000 Rust [I]; unblocks ≤ 9 tests directly, plus most of the 43 `compiletime` tests
  once e exists [V for the counts, I for the dependency].

### c. Transparent inline methods

- dotty: expansion in `adapt` (`T/Typer.scala:4856-4864`: `needsInlining` with `Transparent` or
  `Mode.ForceInline`, `tree.tpe <:< wildApprox(pt)`, `inlineCall`, `readaptSimplified`); the
  `Inlined` node's type is the expansion's, which is the point; `Inliner` first forces the type
  arguments (`N/Inliner.scala:242`); `InlineCall.expand` casts when opaque proxies or skolems are
  involved (`N/Inlines.scala:591-665`); `inlineExpansion` keeps the definition with its body
  (`T/Typer.scala:4079,4163`) [V].
- ours: refused at `inline_refs.rs:16` (call), `namer/checks.rs:106` (definition),
  `interpolate.rs:177` (a transparent result constrained) [V]. `Inlining` needs exactly what
  `Typer` owns (`u`, `tpd`, `names`, `fresh`: `typer/mod.rs:110-121`) [V].
- gap: `Inlining::new` from a `Typer` and a public `inline_call`; in `adapt.rs` after the implicit
  arguments are applied: zonk the call's type variables (dotty's `isFullyDefined`), expand with a
  body from the unit (`Inlining::bodies`) or from d, set the `Inlined` type to the expansion's,
  re-adapt; `checks.rs:106` accepts `transparent`; the oracle in plain mode already prints these
  `Inlined` trees, so `typer-compare` and a golden `tests/typed/transparent.scala` check it.
  Refuse opaque proxies and skolemized arguments (`expand` casts) [I].
- size ~400–600 Rust [I]; alone ≤ 16 tests (13 + 3) [V]; with d ≈ 150 [V, upper bound].

### d. Library inline bodies from TASTy

- dotty: the body is the `DEFDEF` rhs read lazily (`C/tasty/TreeUnpickler.scala:703-710`,
  `LazyBodyAnnotation`, `Inlines.bodyToInline` :46); `readTree` for terms is :1288-1760 (~470
  lines: `readPathTree`, `completeSelect` with signature lookup, `IDENT`…`INLINED`, `readTpt`,
  `readCases`), types assigned by the `tpd` constructors (`TypeAssigner`: `Apply` result,
  `Select` member type, `If`/`Match` lub, `Block` avoidance) [V].
- ours: `crates/tasty` parses every ASTs section structurally (`tree.rs`, terms included), enters
  members and parameters but not method bodies (`symbols/enter.rs:1-3`), keeps `Symbol.tree:
  (FileId, NodeId)` (`symbols/symbol.rs:83`), maps definitions to symbols (`by_tree`,
  `symbols/mod.rs:56`) and decodes types of type trees (`types/decode/mod.rs:51`, `Decoder::ty`);
  `Universe::file(FileId) -> (&Ast, &[String])` (`symbols/mod.rs:122`), `define`/`set_info`/
  `enter` (`symbols/define.rs`) [V]. No term reader exists; the spike printer
  (`tasty/src/print/inline.rs`) renders tags, not trees [V].
- gap: `crates/typer/src/unpickle/` (`terms.rs`, `defs.rs`, `assign.rs`, `locals.rs`): the
  `DEFDEF` rhs at `Symbol.tree` to `tpd` nodes with types (`build.rs` helpers: `apply`, `select`,
  `block`, …, plus `Universe::member_info`/`as_seen_from`/`lub`); local `ValDef`/`DefDef`/`Bind`
  and lambda parameters defined on the fly with an address map; `SHAREDterm` through `Ast::deref`;
  every node gets the call's span (`reposition`); `Inlining::inline_method` falls back to it
  (`I/mod.rs:133-135`). Refuse at first: local classes, `Match`/`Try` until b, quotes, `Inlined`
  inside bodies with bindings. `Predef.assert` (`if !assertion then Scala3RunTime.assertFailed()`)
  and `summon` (`x`) need only `Ident`, `Select`, `Apply`, `If`, `Literal` [I].
- size ~1200–1500 Rust [I]; alone ≤ 2 tests (the two `a call of an inline method` without own
  inline) [V]; with c the ≈150 above.

### e. `scala.compiletime`

- dotty: intrinsics in `InlineCall.expand` (`N/Inlines.scala:504-583`: `requireConst`, `codeOf`,
  `constValue`/`constValueOpt`/`constValueTuple` via `TypeComparer.constValue` (`C/TypeComparer.
  scala:1754,3548`), `summonInline`/`summonAll` via a fresh `Typer.inferImplicitArg`), `error`
  (`N/Inliner.scala:790-808,856`), `summonFrom` (`T/Applications.scala:1447-1475`, an inline match
  with `ImplicitScrutineeTypeRef`, reduced by `reduceInlineMatch` with `isImplicit`), `erasedValue`
  only in reduced scrutinees (`N/InlineReducer.scala:364-376`), `ops.int|string|boolean|any` folded
  by `C/TypeEval.scala` (269 lines) from `TypeComparer:1571` [V].
- ours: refused at `inline_refs.rs:19,25`; `constant_of_type` (`I/constants.rs:67`) is half of
  `constValue`; no `TypeEval`; `inlining::run` has no typer, so no implicit search [V].
- gap: `I/compiletime.rs` for the intrinsics, `tasty/src/types/eval.rs` for `ops`, `summonFrom`
  through b. **Design decision:** `summonInline`/`summonFrom`/`summonAll` need an implicit search
  at the call site after typing. Either run `inlining::run` inside `Units::type_unit` while the
  `Typer` lives, with the `Ctx` of each inline call recorded during typing (a small
  `FxHashMap<TNodeId, Ctx>`, dotty rebuilds contexts by traversal), or expand such calls in the
  typer only. The first keeps the `--inlined` oracle and the plain typer output as they are [I].
- size ~600–900 Rust [I]; unblocks 3–4 tests directly, 20–40 of the 43 `compiletime` tests after
  b–d [I].

## 3. Order of MRs

Each stacked on the previous, each < 1500 lines of diff, gates as in `CLAUDE.md`; before each,
`just snapshot typer lower`, after it `just snapshot-diff` (needs the baseline under
`tmp/dotty/3.9.0-0.5.12/`).

1. **a — wire the phase** (~500 lines). `L/lib.rs`: `typer::inlining::run` before `phases::run`,
   its `Unsupported` mapped to the lowering's; `L/phases/drop_inlined.rs`; `L/phases/erasure/`
   drops inline `DefDef`s; `body.rs:51`/`callee.rs:29` keep refusing what remains; a program
   `tests/programs/inlining/` from `crates/typer/tests/inlined/calls.scala` with a `.check`.
   Checks: `just check`, `just nir-cmp crates/typer/tests/inlined/calls.scala 'Test?'` (positions
   included), `just dotty-lower-report` (+≤13), `just typer-checks` unchanged. Risks: spans of
   proxy bindings and of inlined arguments (`Inlined(EmptyTree, Nil, arg)` keeps the argument's
   span) show in `--debug` NIR; `Inlined` of type `Nothing`/`Unit` at statement position.
2. **c — transparent calls of unit methods** (~500 lines). `Inlining::new(&mut Typer)`, the
   `adapt.rs` hook, `checks.rs:106` and `interpolate.rs:177` lifted, `inline_refs.rs:16` for
   `TRANSPARENT` without `MACRO`. Checks: `tests/typed/transparent.scala` golden
   (`just typer-checks`), `just typer-compare` on `tests/pos/{inline-*,transparent-*}.scala`,
   `just typer-report` not worse. Risks: type variables of the call still open when expanding;
   the expansion's type leaking local symbols (`I/mod.rs:181` refuses this today); `readaptSimplified`
   re-running conversions.
3. **d1 — TASTy term reader, expression subset** (~1200 lines). `crates/typer/src/unpickle/`
   for `IDENT`, `SELECT(in)`, `APPLY`, `TYPEAPPLY`, `TYPED`, `LITERAL`s, `THIS`, `NEW`, `BLOCK`,
   `VALDEF`, `IF`, `LAMBDA`/`DEFDEF` locals, `ASSIGN`, `WHILE`, `RETURN`, `SEQLITERAL`,
   `INLINED` without bindings; `Inlining` body fallback; positions = call span. Checks: `just
   typer-compare --inlined` on scripts calling `assert`, `summon`, `IArray.unsafeFromArray`
   (golden `tests/inlined/library.scala`), `just nir-cmp` on an `assert` program (the call of
   `Scala3RunTime.assertFailed`), `just dotty-lower-report` (expect +100–150). Risks: `SELECT`
   member resolution by signature (`completeSelect`, overloads), `as_seen_from` on the prefix,
   `by_tree` misses for local symbols, erased/`inline$` accessors in bodies, `Scala3RunTime`
   missing from the library image.
4. **d2 — reader, the rest** (~600 lines): `MATCH`/`CASEDEF`/`BIND`/`UNAPPLY`/`ALTERNATIVE`,
   `TRY`, `THROW`, `TYPEDEF` locals refused, `INLINED` with bindings, `NAMEDARG`, `REPEATED`.
   Checks as d1 plus `typer-compare --inlined` on `tests/pos/inline-*` that call library
   methods with matches (`Tuple.size`, `boundary.break` once the typer accepts them).
5. **b — inline if/match** (~1000 lines): tpd flags and printer, typer acceptance,
   `I/reduce/`, `retype.rs` dispatch. Checks: golden `tests/inlined/matches.scala` (the dotty
   `inline match` tests reduced to one file), `typer-compare --inlined` on
   `tests/pos/inline-match*.scala`, `just dotty-lower-report` (+9 and the `compiletime` tests
   that only need `erasedValue` scrutinees). Risks: GADT approximation of type bindings (refuse),
   guard folding order, `$scrutinee` naming for `nir-cmp`, sub-match semantics.
6. **e — compiletime** (~800 lines, after the design decision of §2 e): intrinsics, `TypeEval`,
   `summonFrom`. Checks: golden `tests/inlined/compiletime.scala`, `typer-compare` on
   `tests/pos/compiletime-*`, `tests/run/*compiletime*` through `dotty-lower-report`.
7. Later, small: retained inline methods (17 tests: `bodyRetainer`, erasure keeps them); inline
   accessors (`N/PrepareInlineable.scala:44-250`, 7 tests); `return` from inline bodies
   (`control.rs:115`); `-Xmax-inlined-trees`.

## 4. Risks and open questions

- **One expansion, two hosts.** After c, `Inlining` runs inside the typer (transparent) and in the
  lowering (the rest); e may move the second into the typer too. Keep `Inlining` free of typer
  state so both stay one module [I].
- **Oracle blind spots.** `--inlined` prints only the outermost `Inlined` trees and the plain
  oracle stops after the typer, so the lowering's `drop_inlined` and the erasure of inline
  definitions are checked only by `nir-cmp` and `dotty-lower-report`; every MR of §3 needs a NIR
  comparison, not just a tree one [I].
- **Library bodies are untyped trees.** The reader re-derives every type as `TypeAssigner` does;
  a wrong `Select` type changes erased names silently. The `--inlined` oracle shows the full body
  copy with types, which is the check [V].
- **What `assert` unblocks is an upper bound.** 159 of 314 lower with plain methods; the VM
  then needs `Scala3RunTime` and the `wrong output` rows are unknown until `dotty-lower-report`
  runs with d1 [V for 159, I for the rest].
- **Open:** whether tpd `If`/`Match` take an `inline` field or a `Tree::InlineMatch` variant
  (dotty uses subclasses; the printer needs the class name) [I]; where the recorded `Ctx` for e
  lives (research 09 §2 forbids side tables keyed by `TNodeId` only across speculation) [I].
