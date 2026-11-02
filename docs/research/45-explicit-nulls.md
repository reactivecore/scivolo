# 45 — Explicit nulls for scalalib (#168)

`D/` = `ref/scala3/compiler/src/dotty/tools/dotc/`, `T/` = `crates/tasty/src/`,
`Y/` = `crates/typer/src/`, `SN/` = `ref/scala-native/`, `LIB/` = the sources of
`ref/libs/scala-library-3.9.0-sources.jar`. Context: [research 36](36-compiling-the-native-stdlib.md)
§1.1 and plan row 11; Scala Native compiles scalalib with `-Yexplicit-nulls
-language:implicitConversions -Wconf:any:silent` (`SN/project/Build.scala:521-528`), and only
scalalib: no other module sets the flag (`Build.scala:527` is the single occurrence).

## 1. What `-Yexplicit-nulls` changes in dotty 3.9

1. Switches: `ctx.explicitNulls` reads the setting (`D/core/Contexts.scala:491`); `flexibleTypes` is
   on unless `-Yno-flexible-types` (`:494`); `flexifyTasty` needs `-Yflexify-tasty` (`:497`, off by
   default; settings `D/config/ScalaSettings.scala:575-577`). `Mode.SafeNulls` (`D/core/Mode.scala:179`)
   is set for a unit under explicit nulls and cleared by `import scala.language.unsafeNulls`
   (`Contexts.scala:734-738`); `Nullables.unsafeNullsEnabled = explicitNulls && !SafeNulls`
   (`D/typer/Nullables.scala:27-28`).
2. `Null`'s place: `NullClass` gets parents `Any, Matchable` instead of `Object`
   (`D/core/Definitions.scala:480-484`; forced at compiler start through `syntheticScalaClasses`,
   `:2214`, `:2232-2236`). `isNullableClass` under `SafeNulls` is true only for `Null`, `Any`,
   `Matchable` (`D/core/SymDenotations.scala:900-903`); `TypeComparer.isNullable`
   (`D/core/TypeComparer.scala:994-1017`) decides `Null <: T` and drops the singleton and `this`
   cases under explicit nulls (`:1004`, `:1007`). `isBottomType` is `Nothing` only
   (`D/core/Types.scala:272-274`, `Definitions.scala:1662-1665`). Erasure retracts `SafeNulls`
   (`D/transform/Erasure.scala:775`), so after erasure `Null` is a bottom type again and
   `TypeErasure` is unchanged (`D/core/TypeErasure.scala:414-416`, `:822-823`).
3. `T | Null`: `OrNull(tp)` builds a hard `OrType(tp, Null)` and unapplies through `stripNull`
   (`Types.scala:3788-3793`; `stripNull` in `D/core/NullOpsDecorator.scala:17-48`, 63 lines).
   `widenUnion` keeps a top-level `| Null` (`Types.scala:1454-1462`). Under `unsafeNulls`, member
   selection on `T | Null` selects on `T` (`Types.scala:1007-1015`); the "not a member" error says
   when the member exists without `Null` (`D/typer/TypeAssigner.scala:177-182`).
4. `FlexibleType(lo = T | Null, hi = T)`, a cached proxy type (`Types.scala:3470-3521`): on the right
   it compares against `lo` (`TypeComparer.scala:918-919`), on the left against `hi`
   (`:1132-1133`). 20 more sites look through it: `D/core/TypeApplications.scala:595`,
   `PatternTypeConstrainer.scala:170`, `OrderingConstraint.scala:566,763`,
   `ConstraintHandling.scala:718-721`, `D/typer/Inferencing.scala:628,634`,
   `Typer.scala:1323,1971,1987`, `D/transform/patmat/Space.scala:929,967`,
   `TypeTestsCasts.scala:154`, `Types.scala:322,387,403,428,1476,1907,6160,6519,6832,6981`.
   Printed as `T?` (the oracle prints it so: `scripts/typer-oracle/Sexp.scala:107`).
5. Java members: `ImplicitNullInterop.nullifyMember` (`D/core/ImplicitNullInterop.scala:73-98`,
   272 lines; `JavaNullInterop.scala` no longer exists) maps reference `TypeRef`s and Java type
   parameters of Java-defined members to `FlexibleType` (`| Null` with `-Yno-flexible-types` or
   `@Nullable`), skipping value classes, `Nothing`/`Null`/`Unit`/`Singleton`/`Any`, repeated and
   tuple type constructors, constructor results, given parameters, `@NotNull`, `TYPE`,
   `getClass`, `toString` and enum values (`:76-93`, `:143-174`). Callers:
   `D/core/classfile/ClassfileParser.scala:524`, `D/typer/Namer.scala:1996-1997` (Java sources),
   `D/core/tasty/TreeUnpickler.scala:944-946` (TASTy without the `EXPLICITNULLS` attribute, only
   with `-Yflexify-tasty`).
6. `.nn` is a library extension, `extension [T](x: T | Null) inline def nn: x.type & T`
   (`LIB/scala/Predef.scala:679-681`); `eq`/`ne` on `AnyRef | Null` are extensions beside it
   (`:683`). The compiler only lints it (`UnnecessaryNN`, `D/typer/Typer.scala:1120-1128`).
7. Flow typing (`Nullables.scala`, 636 lines): `NotNullInfo` (asserted and retracted refs,
   `:58-98`), `NotNullConditional` (`:99-118`); `CompareNull` matches `x == null`, `x != null`,
   `eq`, `ne` in both orders, including Predef's extensions (`:120-153`); `TrackedRef`: a stable
   ref, a `@stableNull` field, or a local `var` assigned in the unit and not used "out of order"
   (in a lambda, lazy val, method or class inside its owner; `:156-203`, `:305-318`). The context
   carries `notNullInfos` (`Contexts.scala:806-812`). Typer sites: an ident or select of a tracked
   ref known non-null is cast to `ref.type & T` (`Typer.scala:601-611`, `:709`, `:809`); `&&`,
   `||`, `!` (`Nullables.scala:386-423`); `if` (`Typer.scala:1649-1671`); `while` retracting the
   vars assigned in the loop (`:2475-2483`, `Nullables.scala:549-572`, `assignmentSpans:467`);
   block statements (`:4108`); `val` rhs (`:3103-3104`); `match`: `case null` strips `Null` from
   the scrutinee type for later cases and patterns that cannot match `null` assert the scrutinee
   (`:2307-2310`, `:2340-2342`, `:2415`, `Nullables.scala:208-234`); `try` (`:2554-2569`);
   `return`/labeled (`:2422`, `:2472`); assignment (`Nullables.scala:432-465`); by-name arguments
   (`:574`, `D/typer/Applications.scala:1365`, `:1501`).
8. `unsafeNulls` relaxations: `SafeNulls` off makes `isNullableClass` the after-erasure rule again
   (`SymDenotations.scala:903`, so `Null <: String`); a Java call's `T | Null` is cast away
   (`Typer.scala:5046-5052`); a block with the import inside a safe context is cast to its expected
   type (`:1562-1570`); implicit candidates are ranked without `SafeNulls`
   (`D/typer/Implicits.scala:259`); `matches` too (`Types.scala:1274`).
9. Smaller: `xs*` arguments cast to non-null (`Typer.scala:1339-1345`); `tpd` null tests cast the
   receiver (`D/ast/tpd.scala:1086-1093`); `D/transform/MixinOps.scala:123`,
   `SyntheticMembers.scala:141`; `RefChecks.checkPublicFlexibleTypes` warns on inferred flexible
   result types (`D/typer/RefChecks.scala:1240-1250`); `D/core/TypeOps.scala:227 orDominator` has no
   `Null` case in 3.9.
10. TASTy, erasure, NIR: the pickler writes `EXPLICITNULLSattr` (`D/transform/Pickler.scala:294`,
    `D/core/tasty/Attributes.scala:13,33`, `TastyFormat.scala:285,640`) and `FLEXIBLEtype` (tag
    193, `TreePickler.scala:299-301`, `TastyFormat.scala:183,631`); the unpickler reads
    `FlexibleType(readType())` (`TreeUnpickler.scala:457`). A flexible type erases like its `hi`;
    `T | Null` erases to `T`'s erasure. NIR sees nothing of it.

Sizes: `Nullables.scala` 636, `ImplicitNullInterop.scala` 272, `NullOpsDecorator.scala` 63;
touches in `TypeComparer.scala` 4 lines, `Types.scala` 35 `FlexibleType` lines, `Typer.scala` 9,
`Contexts.scala` 7, `Definitions.scala` 2, `SymDenotations.scala` 1, erasure 1.

## 2. What scalalib exercises

1. Sources: `ref/libs/scala-library-3.9.0-sources.jar`, 649 `.scala` and 57 `.java` files. Patches:
   `scalaVersionDirectories` (`SN/project/Settings.scala:723-737`) applies the `*.scala.patch` files
   (`:900`) of `overrides-3.9.0`, `overrides-3.9` and `overrides-3`; only `overrides-3` exists, with
   10 patches (`Predef`, `Enumeration`, `package`, `Array`, `reflect/Selectable`, `ClassTag`,
   `ScalaRunTime`, `LazyVals`, `ExecutionContext`, `TrieMap`). Research 36's "105" counts across
   versions. The patches add 2 `.nn` and 3 `| Null` lines, no `unsafeNulls`.
2. Idioms (grep): `.nn` 379 uses in 40 files (`mutable/RedBlackTree` 70, `immutable/RedBlackTree`
   67, `CollisionProofHashMap` 64, `ArrayBuilder` 29, …); `| Null` 643 in 91 files; null comparisons
   644 in 89 files; `case null` 103; `= null` 450; `unsafeNulls` import 0. Language imports:
   `implicitConversions` 47 files, `` scala.language.`2.13` ``, `language.experimental.captureChecking`
   in 243 files (the typer has no capture checking; it only matters after typer).
3. Flow typing: sources often add `.nn` even after a null test of a `var`
   (`LIB/scala/collection/mutable/HashMap.scala:182`) and rely on flow typing for stable refs (`:548`).
4. Java members: `System.arraycopy` 67 calls, `Arrays.copyOf` 47, `Arrays.sort` 26, `String.valueOf`
   14, `getClass` 28. On Scala Native `java.*` is javalib, whose TASTy (`libs/javalib-tasty-0.5.12.jar`)
   carries only `SOURCEFILE`: no `EXPLICITNULLS`, no `JAVA` attribute, so dotty keeps their declared
   types. javalib on the class path shadows the JDK's class files: only the classes javalib defines
   under other names (`String`, `Object`, `Comparable`, …, `T/symbols/jdk.rs` `RENAMED`) come from the
   JDK, so `"a".substring(1)` is `String?` while `new StringBuilder().append("a")` and
   `System.getProperty("a")` stay `String`-typed (probes `java-members.scala`, `flexible.scala`).
5. The published `scala-library-3.9.0.jar` TASTy is compiled with explicit nulls (attributes
   `SCALA2STANDARDLIBRARY, EXPLICITNULLS, CAPTURECHECKED, WITHPUREFUNS, SOURCEFILE`): `| Null` members
   and `FLEXIBLEtype` already reach our unpickler today.

## 3. What our code does today

1. `Null` is a synthetic class with parent `Object` (`T/symbols/builtin.rs:77`, `:97`). `Null <: T` is
   decided in `fourth_try` by `nullable(b)` (`T/subtype/fourth.rs:18`, `T/subtype/null.rs:16-50`: the
   rule without explicit nulls); the base-type path (`T/subtype/first.rs:133,147`) answers
   `Null <: AnyRef` through the parents. Bottom tests: `T/subtype/join.rs:173-177`,
   `T/subtype/refs.rs:64`, `Y/typer/patterns.rs:113-118`. `widen_union` is "without explicit nulls"
   (`Y/typer/widen_union.rs:19-21`). Erasure handles `Null` beside a reference
   (`T/erasure/bounds.rs:91-96`, `:177-183`).
2. `FLEXIBLEtype` decodes to `Type::Flexible(hi)` (`T/types/flexible.rs`); JDK members are nullified
   under explicit nulls in `T/symbols/java_nulls.rs`.
   TASTy attributes are parsed (`T/file.rs:62-88`), only `SCALA2STANDARDLIBRARY` used.
3. `.nn`, `eq`/`ne`: ordinary Predef extensions. No `unsafeNulls`, no `SafeNulls` mode, no not-null
   tracking, no `case null` stripping.
4. Options: `Config::from_options` (`crates/syntax/src/config/options.rs:29-100`); `-Yexplicit-nulls`
   is unknown. `Features` is a `u16` with all 16 bits used (`crates/syntax/src/config/mod.rs:16-34`);
   `feature_enabled` walks import levels (`Y/typer/features.rs:12-36`).
5. Per-unit mode: the flag lives in `Config` → `Typer` (`Y/typer/mod.rs:255-277`) and in `Cmp`
   (`T/subtype/mod.rs:121-146`, beside `frozen`, `necessary`, `provisional`; callers `:87`, `:92`,
   `:105`). Gates only on the `Null`-class paths; `just typer-report` is the regression guard.
6. The oracle: `typer-compare` passes `//> using options …` to both sides (`scripts/typer-compare.py:37-44`,
   `scripts/typer_corpus.py:11`); `Oracle.scala` builds the compiler before distilling options
   (`newBase`, `:49-56`), but `NullClass` is forced at `initialize()`, so `--fresh` must distill the
   options into the root context before `initialize()`.

## 4. Plan

Each batch is accepted by `just typer-compare` on probe files under `tests/explicit-nulls/pos/` whose
first line is `//> using options -Yexplicit-nulls --fresh`, plus an unchanged `just typer-report`.

1. **Mode, subtyping, oracle** (M, ~350). `Config.explicit_nulls` from `-Yexplicit-nulls` (refuse
   `-Yno-flexible-types`, `-Yflexify-tasty`); `Features` to `u32` with `UNSAFE_NULLS`; `Typer` and
   `Cmp` flags; `Null <: T` only for `Null`/`Any`/`Matchable`, unions and bounds of them (`null.rs`,
   `fourth.rs`, `first.rs`); `is_bottom_type`; `widen_union` keeping `| Null`; `Oracle.scala`
   distilling before `initialize()`; `typer_corpus.py` for the new directory. Probes:
   `null-basics.scala`, `java-members.scala` (settles §2.4). Files: `options.rs`, `config/mod.rs`,
   `null.rs`, `fourth.rs`, `first.rs`, `subtype/mod.rs`, `patterns.rs`, `widen_union.rs`,
   `Y/typer/mod.rs`, `Oracle.scala`, `typer_corpus.py`.
2. **Flexible types** (M–L, ~500; only if batch 1's probe shows `String?`). `Type::Flexible(hi)`
   kept by `decode/mod.rs:127`; subtyping lo/hi sides, `strip_null`, `widen`, erasure through `hi`,
   printing `T?`, `ImplicitNullInterop` for the JDK declarations (`T/symbols/jdk.rs`, `runtime_stubs.rs`).
3. **Nulls in patterns, selections and `unsafeNulls`** (M, ~350). `case null` strips the scrutinee
   type (`Y/typer/matches.rs`), patterns that cannot match null, `xs*` casts (`repeated.rs`), the
   "not a member without Null" hint, the `unsafeNulls` import (`features.rs`, `imports.rs`) with its
   relaxations (after-erasure nullability in `Cmp`, selection on `T | Null`, Java-call casts in
   `adapt.rs`, block cast).
4. **Flow typing, straight-line** (L, ~600). New `Y/typer/nullables.rs` (`NotNullInfo`, conditionals,
   `CompareNull`, tracked refs: stable refs and local vars assigned in the unit and not captured) and
   `nullables_ctx.rs`; casts at idents and selects, `&&`/`||`/`!`, `if`, blocks, `val` rhs.
5. **Flow typing, loops and matches** (M, ~450). `while` with retraction, `match` case contexts,
   `try`/`finally`, `return`/labeled, assignments, by-name arguments, `usedOutOfOrder`.

Refusable (per file, naming the shape): `@stableNull` fields, not-null tracking through by-name
arguments and into lambdas, the `UnnecessaryNN` and `checkPublicFlexibleTypes` warnings,
`-Yno-flexible-types`, `-Yflexify-tasty`. Not refusable: `Null`'s subtyping, `| Null` inference, `.nn`,
`case null`, `if x != null` on stable refs and local vars.

Cost note: after erasure nothing differs (§1.10). A file typed with the flag off yields the same NIR
wherever typing succeeds; `typer-compare` shows where typing differs, `lib-compile`'s NIR comparison
is the final arbiter per file.
