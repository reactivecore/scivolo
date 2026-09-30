# 09 — A typer for the scripting subset

Date: 2026-09-30. Claims are [V] verified by reading the code or running it, or [I] inferred.
Paths are relative to `ref/dotty` (tag 3.9.0; `T/` = `compiler/src/dotty/tools/dotc/typer/`, `C/` =
`.../dotc/core/`, `N/` = `.../dotc/inlines/`); the tasty crate is read at `28-subtyping` (c1a5716,
MR !37). ROADMAP phase 3, step 3. Follows [research 07](07-desugar-port.md) and
[08](08-tasty-symbols.md); the prototype oracle lives in `tmp/typer/`.

**Recommendation in one paragraph.** Port dotty's typing *algorithms* faithfully — `Typer.typed*`
with `adapt`, `Applications`, `Implicits`, `ProtoTypes`/`Inferencing`, and `TypeComparer` with
`OrderingConstraint` — restricted to the scripting subset, into a new crate `crates/typer` that
consumes `crates/syntax` trees and defines script symbols and types inside the one `tasty::Universe`.
A "simplified typer" is not an option: the lowering must pick the same overloads, givens and
inferred (then erased) types as dotc to produce nscplugin's names (ADR 0001), and every difference
is observable. Leave out match types, GADT reasoning, capture checking, macros, `Dynamic`, explicit
nulls and `derives`: each raises `Unsupported` and the script takes the phase 1 path with a notice.
Test differentially against a dotc-to-`typer` oracle that prints typed trees with types and symbols
as S-expressions; the prototype types 3363 of 3376 single-file `tests/pos` files in 33 s (2 ms
median per file, warm), 3151 of them inside the subset [V].

## 1. What dotty's typer is

Lines in 3.9.0 [V]; "subset" says what the scripting subset ports (Scala lines).

| part | lines | subset |
|---|---:|---|
| `T/Typer.scala`: `typedUnadapted` dispatch (:3833), 80 `typed*` methods, `findRef` and imports (:220-460), `adapt` (:4428-4990) | 5,421 | ~3.5k: all `typed*` except quotes/splices, `Hole`, `Labeled`, `Dynamic`, migrations |
| `T/Namer.scala`: `index`/`enterSymbol`, `Completer`/`ClassCompleter` (:862-1900), signatures (:1907-2330) | 2,332 | ~1.5k: symbols entered up front, infos completed lazily |
| `T/Applications.scala`: `Application.matchArgs` (named, default, varargs :533-1100), `typedApply` (:1298), `typedUnApply` (:1718), `resolveOverloaded` (:2431-2800), `compare` (:2159), `harmonize`, extension application (:2982) | 3,062 | ~2.5k: all except `spread`, `Dynamic` |
| `T/Implicits.scala`: `ImplicitSearch` (:1274-1970), `ContextualImplicits`, `OfTypeImplicits`/`implicitScope` (:826), `inferView` (:903), ranking by nesting level | 2,227 | ~1.5k: no suggestions, no by-name implicit dictionaries (:1981) |
| `T/ProtoTypes.scala` (`FunProto`, `SelectionProto`, `ViewProto`, `PolyProto`, `constrained`, `wildApprox`), `T/Inferencing.scala` (`interpolateTypeVars` :663, `instantiateTypeVars` :712, `variances` :554), `TypeAssigner`, `EtaExpansion`, `ConstFold`, `ImportInfo` | 3,434 | all but `approximateGADT`, quotes |
| `T/Checking.scala` (69 checks), `T/Synthesizer.scala` (`ClassTag`, `TypeTest`, `ValueOf`, `Mirror`, `CanEqual`, manifests) | 2,701 | checks that change *acceptance* (bounds, stability, irrefutability, cycles); `ClassTag`, `TypeTest`, `ValueOf`; not `Mirror` (`derives`), manifests |
| `T/RefChecks`, `VarianceChecker`, `CrossVersionChecks` (phases after typing [V]), `Nullables`, `Dynamic`, `Deriving`, `QuotesAndSplices`, `ImportSuggestions`, `ReTyper`, misc | 5,657 | `ReTyper` (for the inliner) only |
| `C/TypeComparer.scala` (`recur` :255-1665, `isSubArgs`, `hasMatchingMember`, `either` :2018, `glb`/`lub` :2542-2700, `MatchReducer` :3644, `provablyDisjoint` :3063) | 4,113 | ~1.5–2k: no `MatchReducer`, GADT bounds, captures, `necessaryEither` |
| `C/OrderingConstraint`, `ConstraintHandling`, `Constraint`, `TyperState` | 2,489 | ~1.2k, with snapshot/rollback for `tryEither` |
| `C/Types`, `TypeOps`, `TypeApplications`, `Substituters`, `Definitions`, `Mode`, `Scopes` | 13,448 | the type model is `crates/tasty` (08 §4); `avoid`, `simplify`, `appliedTo`, ~150 `defn.*` names, `Mode` bits |
| `C/GadtConstraint`, `PatternTypeConstrainer`, `TypeEval` (compile-time ops) | 861 | pattern constraints on type *variables* only |
| `N/Inlines`, `Inliner` (`InlineTyper` :899), `InlineReducer`, `PrepareInlineable`; bodies are the TASTy `DEFDEF` rhs (`C/tasty/TreeUnpickler.scala:704`, `LazyBodyAnnotation`) and need a `TreeUnpickler` (1,971) | 2,780 | yes, after the core (§5 MR 7) |
| `cc/` (10,184), `quoted/` (1,124) | 11,308 | no (capture checking never, quotes in phase 4) |

Total 24,835 lines in `typer/`, 37,195 in `core/` [V]. Normative text: SLS ch. 3 (conformance,
`baseType`, `asSeenFrom`, member types: `docs/_spec/03-types.md:1255-1560`), 6.26 (`06-expressions.md:
1088-1430`: conversions, overloading, local type inference, eta-expansion), ch. 7 (implicits), and the
reference pages `changed-features/{implicit-resolution,overload-resolution,implicit-conversions-spec,
eta-expansion-spec}.md`, `contextual/*.md`, `metaprogramming/inline.md` [V]. The spec only says that
inference picks a "minimal or maximal" solution and never a singleton (:1300-1310); widening of
unions and singletons, harmonization, `widenInferred` and the nesting-level rules
(`implicit-resolution.md` rules 2, 7-9) exist only in the compiler, so the compiler is the contract
and the oracle its executable form [V].

Why the subset still needs all of this: of the 3363 typed `tests/pos` files, 1417 have inferred type
arguments, 1026 reference overloaded members, 699 have closures, 604 define givens, 563 have
inferred `using` arguments, 377 apply implicit conversions, 366 mention union and 255 intersection
types, 191 call extension methods, 232 expand inline calls; 167 use match types and 9 quotes [V, §4].
`moderate.scala` alone needs 40 overloaded references, 19 closures, 21 inferred type arguments, 7
`using` arguments, a conversion, default arguments, union types in 9 trees and `f"…"` [V].

## 2. Port strategy and representation

- **Faithful algorithms, own machinery.** Functions follow dotty one to one (as the parser and
  desugaring do), so an oracle diff maps to a dotty line. Not ported: `Denotation`s, periods,
  `Context` chains, mutable `TypeVar`s. dotty's `Context` becomes an explicit `Ctx { owner, scope,
  mode, imports, locked vars }`; scopes and imports are arenas in the typer [I].
- **Trees.** `crates/typer/src/tpd/`: a `Tree` enum of dotty's 48 `tpd` node classes (`ast/Trees.scala`)
  with `tpe: TypeId`, `sym: Option<SymbolId>` on `RefTree`/`DefTree` and a span, in an arena per
  unit (`TNodeId`, list pool as in `syntax::Ast`). Typing *translates* `syntax::Ast` nodes into it
  (dotty rebuilds too, `cpy`); the untyped tree stays immutable. Speculation (`tryEither`,
  `tryInsertApplyOrImplicit`, `typedOpAssign`) marks arena, constraint and diagnostics and
  truncates on rollback; side tables keyed by `TNodeId` are not allowed [I].
- **Types and symbols live in `tasty::Universe`**, script ones included, so that `member`,
  `base_type`, `as_seen_from` and `erase` work uniformly on `class Circle extends Shape` and on
  `Ordering[Int]`; a typer-side `Sym::Lib | Sym::Src` would double every type operation. Needed:
  `define(Symbol) -> SymbolId`, `enter(owner, sym)`, `set_info`, `set_parents`, `set_flags`, and
  `Symbol.tree` generalized to `origin: Tasty(FileId, NodeId) | Source(UnitId, syntax::NodeId) |
  Synthetic`. Completion stays in the typer: Namer enters every symbol of a unit with `Pending`
  info, and `Typer::info_of(sym)` completes it (dotty's `Completer`, `typedAhead`) before any
  `Universe` query can reach it; `Completing` marks cyclic references [I].
- **Type variables.** `tasty::Type` gains `Var(VarId)`, `Wildcard(Option<TypeId>)` (`wildApprox`,
  `?` arguments) and `Error`; prototypes (`FunProto`, `SelectionProto`, `ViewProto`, `PolyProto`,
  `IgnoredProto`) stay a typer enum `Proto` and never nest in types. Instances live in the typer's
  constraint; a type is `zonk`ed (instantiated vars substituted with `map_type`) before member
  selection and, once all vars of a top-level definition are instantiated, over its typed nodes
  (what `Pickler` does with `TypeVar`s) [I].
- **What `crates/tasty` has** (c1a5716) [V]: `open`, `package`, `decl(s)`, `member`, `member_where`,
  `resolve`, `module_class`, `symbol`, `full_name`, `binary_name`, `info`, `types().get`,
  `intern_type`, `show`, `parents`, `linearization`, `derives_from`, `class_symbol`, `erase`,
  `method_signature`, `bridge_signatures`, `base_type`, `as_seen_from`, `member_info`,
  `type_params` (class or method), `class_param`, `normalize`, `matches`, `param_clauses`,
  `result_type`, `map_type`, `subst`, `expand`. Variance, `HASDEFAULT`, `INLINE`, `GIVEN`,
  `EXTENSION`, `MUTABLE` are TASTy modifier bits on `Symbol.flags`, parameters are a method's
  `decls`, `T*` is `Applied(<repeated>, [T])`.
- **What it needs in addition** [I]:
  1. Step 5b, `is_subtype`, specified below.
  2. `widen(t)` (`TermRef`/`Constant`/`ByName` → underlying, recursively) and a `dealias(t)` that
     *keeps* prefixes (`normalize` drops them, losing path dependence in `x.T`).
  3. `member_of_type(t, name) -> Vec<SymbolId>` = dotty's `Type.member`: through `This`, `TermRef`
     (widen), `Applied`/`Sym` (class), `And` (both sides), `Or` (the join's class), `Refined` (the
     refinement first), type parameters and abstract types (upper bound).
  4. `lub`/`glb` on ground types (`TypeComparer:2542-2700` without `orDominator`); the typer widens.
  5. `Type::Constant` with a string payload (literal types of script strings; a `NameRef` is
     file-local).
  6. Later, `NameId` instead of `String` in `TypeRef`/`TermRef`/`Method.params`: `intern_type` clones
     strings today; a performance item.
  7. Step 7 as in 08 §6, `body(def) -> Option<(FileId, NodeId)>`; the typed-tree decoder (TASTy →
     `tpd` arena) lives in the typer.

**`is_subtype` (step 5b).** One comparer for ground types and for constraint solving, as in dotty,
with the constraint behind a trait so that `crates/tasty` holds no typer state:

```rust
pub struct VarId(pub u32);                         // Type::Var(VarId)
pub struct Mark(usize);
pub trait TypeVars {
    fn instance(&self, v: VarId) -> Option<TypeId>;                          // substitute if Some
    fn add_lower(&mut self, u: &mut Universe, v: VarId, lo: TypeId) -> bool; // false: contradiction
    fn add_upper(&mut self, u: &mut Universe, v: VarId, hi: TypeId) -> bool;
    fn bounds(&self, v: VarId) -> (TypeId, TypeId);                          // used when frozen
    fn frozen(&self) -> bool;                                                // compare, never add
    fn mark(&self) -> Mark;                                                  // for `either`
    fn rollback(&mut self, m: Mark);
}
pub struct NoVars;                                 // ground: a Var equals only itself
impl Universe {
    pub fn is_subtype(&mut self, a: TypeId, b: TypeId) -> bool;             // = _in with NoVars
    pub fn is_subtype_in(&mut self, a: TypeId, b: TypeId, vars: &mut dyn TypeVars) -> bool;
    pub fn is_same_type_in(&mut self, a: TypeId, b: TypeId, vars: &mut dyn TypeVars) -> bool;
    pub fn lub(&mut self, a: TypeId, b: TypeId) -> TypeId;
    pub fn glb(&mut self, a: TypeId, b: TypeId) -> TypeId;
}
```

Rules in dotty's order (`recur`: `firstTry`…`fourthTry`) [V for the rules, I for the cut]: identity
of ids; `b` = `Any`/`AnyKind`, `a` = `Nothing`, `Null <:` reference types; `Var` on either side →
`add_lower`/`add_upper` after `instance`, against `bounds` when frozen; named types with the same
symbol and `is_sub_prefix` (spec 3.5.1); aliases through `expand`/`dealias`; abstract type or
parameter on the left → its upper bound, on the right → its lower bound (fourth try); singletons:
left widens, right needs `is_same_ref`; `Applied` with the same class → `is_sub_args` by variance
with wildcard and bounds arguments, else `base_type(a, class(b))`; type lambdas by parameters, class
constructors eta-expanded (`liftIfHK`); `And`/`Or`: the four rules, `Or` on the right and `And` on
the left through `either` = try, `mark`, `rollback` (`sufficientEither`; `necessaryEither` is
GADT-only); `Refined` via `has_matching_member` on `member_of_type`; `Rec`/`RecThis`;
`Method`/`Poly` with `matches`-style parameter equivalence and covariant results; `ByName`;
`Annotated` dropped; constants widen to their class unless the right side is a singleton; `Match`:
left as its bound, right → `false` (the typer reports `Unsupported`); `java.lang.Object` ≡
`AnyRef`/`Any` for Java members (`FromJavaObject`); a `pending` set for F-bounded recursion and a
depth limit. The typer implements `TypeVars` by porting `OrderingConstraint`: bounds per variable
(`lo | bound`, `hi & bound`), ordering between variables, propagation, satisfiability through
`is_subtype_in` while `frozen`, snapshots by cloning (the constraint is small; dotty's is
persistent); nesting levels and `LevelAvoidMap` are left out at first, `TypeOps.avoid` runs at
block exits instead.

## 3. The desugarings deferred from 07 §5 step 6

Each goes where dotty has it, tree for tree, in `crates/typer` [V for the places]:
- `makeClosure`, `makeTupledFunction`, `patternsToParams`, n-ary `makeCaseLambda`: `typedFunctionValue`
  (`Typer:1857-2090`) from the expected type's arity and `Product`-ness; MR 3.
- `makeContextualFunction`: `adapt` (:4024); `makePolyFunctionType`: `typedPolyFunction` (:2092); MR 3.
- named tuples (`tuple` with names, `adaptPatternArgs`/`checkWellFormedTupleElems` `Applications:226-249`):
  `typedTuple` (:3762) and unapply typing; MRs 4 and 6.
- `makeTryCase`: `typedTry` (:2572), needs "the handler is a `PartialFunction`"; MR 4.
- `typedAssign`/`typedOpAssign` (`update`, setters, `x += y` after `+=` fails, `Applications:1514`); MR 4.
- `refinedTypeToClass`: `typedRefinedTypeTree` (:2704); `derivedTree` of `DerivedTypeTree`s (D:101-178)
  when the watched parameter is typed; `invalidateIfClashingSynthetic`, `interpolatedEnumParent`
  (ground bounds) in Namer; `MainProxies` (`Typer:3652`, 125 lines) after a unit's top-level
  statements; MR 2. `quotedPattern`: phase 4.

## 4. Differential testing

- **Oracle** (`tmp/typer/oracle/Oracle.scala`, scala-cli on `scala3-compiler` 3.9.0; runner
  `tmp/typer/run.py programs|pos|posdirs|sn`) [V]: one resident `ContextBase` + `Compiler`; per job (a
  file, or a directory's files) a fresh context with the job's `//> using options` and
  `-Ystop-after:typer`, `compiler.newRun.compile(files)`, then each unit's `tpdTree` printed as
  `(Class span [sym="full.name#(erased,params)"] [flags] :type fields)` with UTF-8 offsets and
  structural types (`t:scala.Predef$.println`, `T:scala.List[T:scala.Int]`, `(x: T)R`, `(A | B)`,
  instantiated `TypeVar`s as their instance), per file its diagnostics and feature counts
  (`== file-stats`). Residency needs three fixes: the run's top-level symbols are deleted from
  their package scopes afterwards (`ClassDenotation.delete`), else later files see them; `package
  scala` files and jobs with another class path (`-classpath` is fixed at `initialize`) get a
  throwaway base (`--fresh`); a crash rebuilds the base (`== reset`, 10 of 3376 files). Rejected:
  `-Vprint:typer -Xprint-types` (text, no symbols or spans); a fresh base per file (~0.3 s each).
- **Numbers** [V]: `tests/pos` single files: 3376 jobs, 3363 typed without error, 3 with errors
  (need sibling files), 10 crashed (stale symbols); 32.8 s wall including JVM start, median 2 ms,
  p90 7 ms, max 8 s per file; 41 MB. `tests/pos` directories (without `.java`): 178 jobs, 445 files,
  429 clean, 16 with errors (duplicate `Test` objects, `-Werror`, `Mirror` derivations); 12.1 s.
  `tests/programs`: 13 of 14 (`overflow` needs nativelib on the class path). Scala Native's language
  tests (`unit-tests/shared/.../scala/*.scala` with the `utils` files, JUnit and nativelib): 26 files,
  22 clean, 4 need sbt's `BuildInfo` or javalib test utilities.
- **Corpora for the crate**: (1) `tests/programs` and the M2 language tests, the target; (2) the 3151
  `tests/pos` single files inside the subset (no match types, quotes, macros, capture checking,
  explicit nulls, `-language:experimental`), a file passing when its S-expression matches after
  normalization (fresh names, `Inlined` calls, synthetic spans); (3) `tests/neg` later, for the
  errors the typer reports itself. M3–M5 sources are the end-to-end check of ROADMAP step 5, not a
  tree oracle.
- **Coverage of the subset**: the feature table per corpus (a feature is covered when its files
  pass), the pass count per MR on corpus (2), and the *fallback rate* on M2–M5 sources
  (`just typer-report`), the number that has to reach zero.
- **Format contract** for the Rust printer: `full_name` + `#(` the erased parameter type names as
  dotty's `Signature` spells them (`java.lang.Object`, `scala.Int`, `java.lang.String[]`, from
  `Erased`) `)`; tree types *unwidened* (`t:x` for a `val` reference), so the typed arena keeps
  dotty's `tree.tpe` exactly [I].

## 5. Order of work, fallback, sizes

Stacked MRs, each gated by the oracle pass counts (Rust lines, [I]):
1. Skeleton (~2.5k): `crates/typer` with the `tpd` arena and printer, `Ctx`/scopes/modes/imports,
   the `Universe` additions of §2 (`define`, `Var`, `Wildcard`, string constants),
   `scripts/typer-oracle` + `scripts/typer-tests.py` from `tmp/typer` with `just typer-oracle|
   typer-report|typer-diff`; literals, blocks, `Predef` selection and `println` for `hello.scala`.
2. Namer (~2.5k): entering and completing units, classes, objects, package objects, imports and
   `findRef` precedence, `typedClassDef`/`typedDefDef`/`typedValDef`, parents and `new`, module
   references, the Namer-side items of §3.
3. Applications and inference (~4k): `FunProto` and argument typing, type variables and the
   constraint (tasty 5b lands here), `interpolateTypeVars`/`instantiateTypeVars`/`widenInferred`,
   `TypeApply`, closures with inferred parameter types, eta-expansion, SAM conversion, `avoid`.
4. Applications, second half (~2k): overload resolution and `compare`, named and default arguments,
   varargs, harmonization, assignments and `+=`, tuples and named tuples, `try`, interpolators.
5. Implicits (~2.5k): contextual and of-type scopes, search, ranking by nesting level, `using`
   arguments, conversions (Predef's included), extension methods, givens, `ClassTag`/`TypeTest`/
   `ValueOf` synthesis, `NotGiven`.
6. Patterns (~1.2k): `typedUnApply`, typed/literal/alternative/bind patterns, type-variable
   constraints from patterns, irrefutability, sealed and enum checks.
7. Inline (~3k): TASTy bodies to `tpd`, `Inliner` with `InlineTyper`/`ReTyper`, `inline if`/`match`,
   `summonInline`, `constValue`/`erasedValue`; needed for `summon`, `IArray`, `boundary.break`.
8. Hardening (~1.5k): `tests/neg` parity for the errors the typer keeps, opaque types, exports, lazy
   vals, unions and intersections as the corpus needs, fallback plumbing in `scalai`.

15–19k lines of Rust in `crates/typer` plus 1.5–2k in `crates/tasty` (5b and §2 items 2–5); with
the lowering still to come, the largest part of phase 3's 9–15 person-months [I].

**Fallback.** `typer::type_unit` returns `Result<Unit, Fallback>`: `Unsupported { at, what }` (match
type, macro or quote, `Dynamic`, `derives`, structural call, capture-checking or experimental import,
a `java.*` member javalib's TASTy lacks, GADT bounds from a pattern) or `Errors(diags)`. `scalai` then
runs the phase 1 path (`crates/scalai/src/script.rs`: scala-cli behind the content-hash cache) and
prints one line on stderr, `note: <file>:<line>: falling back to scala-cli (<what>)`. `Errors` falls
back too, because a type error from a subset typer may be the typer's bug: dotc's diagnostics are
the ones shown, and if dotc compiles what we rejected the note says so. `SCALAI_FRONTEND=strict`
turns fallback into an error (tests, CI), `=jvm` forces the old path. The own frontend's NIR also
enters the content-hash cache, so a hit costs VM time only [I].

**Budget for ≤ 50 ms** (`moderate.scala`, M4 Pro): parse + desugar 3.9 ms as a process
(`syntax-dump desugar`, median of 20 runs) [V]; TASTy for the ~60 files the script touches at
spike 01's rate (941 files in 12.7 ms) ≈ 1–2 ms plus a TASTy image open (ADR 0005-style) ≈ 1 ms [I];
typing 5–15 ms [I] (warm dotc: 2 ms median per pos file, 13 ms for `hello`); lowering ≈ 5 ms [I];
VM start and run 17.4 ms [V, ADR 0005]. About 35–45 ms: inside the criterion, with no room for a
typer 3× slower than estimated.

## 6. Risks and open questions

- **Inference details** (widening, harmonization, avoidance, `either` order) change erased signatures
  and thus mangled names, silently. The oracle compares whole typed trees, and the lowering's names
  are checked again against nscplugin (ROADMAP step 5) [I].
- **Speculative typing** must roll back arena, constraint, diagnostics *and* symbol definitions
  (Namer runs inside `typedBlock`); a leak becomes a ghost in later lookups [I].
- **`java.*` types** occur in every unit (each template calls `java.lang.Object.<init>`): javalib
  TASTy (08 §3 B) is on the critical path of MR 2. Scripts using JDK members javalib lacks fall back
  and fail at link time on the phase 1 path, as today [V].
- **Inline is not optional**: `summon`, `IArray`, `boundary`, `compiletime` are inline and 7% of pos
  files expand inline calls; until MR 7 those scripts fall back [V].
- **Post-typer rewrites the lowering inherits**: `f"…"` (`transform/localopt/FormatChecker` +
  `StringInterpolatorOpt`, 547 lines), `SyntheticMembers` (740) for case classes and enums, the
  `@main` proxies' `CommandLineParser` calls; all needed for `moderate` [V].
- **Open**: whether `Type::Var` in the shared interner suits the tasty owner or a typer-local
  wrapper is preferred [I]; how much of `Checking` the subset can drop when every rejected program
  falls back anyway (only checks that change the *typing*, e.g. `checkStable` on import prefixes
  and `checkBounds` on inferred arguments, are needed for acceptance) [I]; nesting levels: dotty's
  `avoid` at block exit was replaced by level checks in 3.2, so oracle diffs on local classes in
  inferred types will show whether levels are needed [I].
