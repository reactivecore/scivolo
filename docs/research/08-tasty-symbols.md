# 08 — Symbols and types from TASTy

Date: 2026-09-29. Claims are [V] verified by reading the code or running it, or [I] inferred.
Issue #28; ROADMAP phase 3, step 2. Reference: tasty-query at eee1c9b (`ref/tasty-query`,
paths below relative to `tasty-query/shared/src/main/scala/tastyquery/`), dotty 3.9.0
(`ref/scala3`), Scala Native 0.5.12, spike 01 (`spikes/01-tasty-reader`).

**Recommendation in one paragraph.** Build `crates/tasty` from spike 01 into a read-only model of
the library's symbols and types in tasty-query's shape: packages known up front, a top-level
file decoded when a name in it is first looked up, cross-file references kept as names until
used, parents/linearization/signatures/erasure computed and memoized on first use. Everything
lives in arenas owned by one `Universe`, queried and completed through `&mut Universe` (no
`Rc<RefCell>`, no globals). Port in slices that each have an oracle: erased signatures first,
checked against the mangled names of the Scala Native jars, which needs no JVM; member lists,
types and subtyping later, against golden dumps from tasty-query. The typer's own machinery
(type variables, overload resolution, implicit search, inference) is not in tasty-query and is
phase 3 step 3.

## 1. tasty-query in numbers [V]

18,806 lines of Scala in `shared/src/main`. What a Rust port would take, by part:

| part | lines | needed |
|---|---:|---|
| Symbols, flags, annotations | 2,324 | yes |
| Types, constants, names | 2,991 | yes |
| Type maps, substitution, type ops, subtyping, type matching | 1,916 | yes, match types later |
| Erasure, signatures | 506 | yes (Scala 2 erasure: no) |
| Loading, classpath | 538 | yes, own cache |
| TASTy reader (tree unpickler, format) | 3,236 | partly in spike 01 |
| Java classfiles | 1,651 | only for option A in §3 |
| Scala 2 pickles | 1,849 | no (the stdlib is TASTy since 3.8) |
| Trees, printers | 2,553 | bodies later (inline); printer for dumps |

The rows marked "yes" are 8,275 lines without the TASTy reader, 11,511 with it [I: ~10–14k lines
of Rust]. dotty's own versions are larger (`Types.scala` 7,318 against 2,457;
`TypeComparer.scala` 4,113 against `Subtyping.scala`'s 572).

## 2. How tasty-query loads [V]

- **Up front:** every package symbol, with the raw bytes of its classpath entries; no file is
  decoded (`Loaders.scala:226`).
- **On a miss:** `PackageSymbol.getDecl(name)` loads the one root the name belongs to
  (`Symbols.scala:1900`, `Loaders.scala:70`); listing a package's declarations loads all of its
  roots. A re-entrant load is an error ("Cyclic loading").
- **Per file:** decoded whole: all symbols first (forward references), then typed trees of all
  bodies (`TreeUnpickler.scala:47–187`); `SHAREDtype` results cached by address.
- **Lazy after loading:** references to other files stay names until resolved; parents,
  linearization, signed names, erasure, base types are computed and memoized on first query.
  So loading one file never loads another.
- **Overloads** are told apart by the signed name (name plus erased signature), which is what
  TASTy stores at call sites. Member lookup walks the linearization and returns the first
  match; there are no overload sets.

For us: decode member *signatures* eagerly per file and method *bodies* on demand (bodies are
needed only for `inline` expansion); spike 01's arena for the whole stdlib would be ~33 MB.

## 3. Where symbols come from [V]

| library | TASTy | NIR | notes |
|---|---|---|---|
| Scala standard library | `scala-library-3.9.0.jar` (941 files) | `scalalib_native` | the native jar strips TASTy |
| nativelib, clib, posixlib | yes | yes | TASTy 28.1 (Scala 3.1.3), decodes |
| javalib, auxlib | **no** | yes | Scala Native keeps only NIR (`NIROnlySettings`) |
| JDK API (`java.*`) | — | — | what scala-cli compiles scripts against today |

Scripts call `java.*`. Two ways to know those types without a JVM:

- **A. JDK class files.** Read `lib/ct.sym` (or `jrt:/`) of an installed JDK, as dotc does for
  `-release`; port tasty-query's classfile reader (1,651 lines). Types match what scripts are
  compiled against today. A JDK installation becomes a data dependency of the frontend, and
  the JDK offers methods javalib lacks, which then fail at link time instead of in the typer.
- **B. javalib's TASTy.** Build javalib (and the overlay, ADR 0009) with TASTy kept and cache it
  with the other libraries. Types follow what the VM can run; no JDK at run time; no
  classfile reader. What it takes:
  - One Scala Native build per version, with sbt and a JDK: `NIROnlySettings`
    (`project/Settings.scala:694`) only drops `.class`/`.tasty` from the packaged jar, so
    `javalib3/compile` leaves the TASTy of all 602 javalib sources in `target/…/classes`.
  - The names nscplugin maps when it writes NIR (`NirGenName.scala:168–180`): javalib defines
    `java.lang._String`, `_Enum`, `_Comparable`, `_Cloneable`, `java.io._Serializable`,
    `_NullPointerException`, `annotation._Retention`; `Object` and `Class` are nativelib's
    `private[runtime] _Object` and `_Class`. The frontend applies the same mapping.
  - javalib is compiled against the build JDK, so its TASTy refers to `java.lang.String` and
    others by name; with the mapping those resolve to javalib's own classes.
  - It differs from the JDK where javalib is incomplete: a script using a missing JDK method
    fails in the typer, the earlier, clearer error.

Recommendation: **B**, with A as a fallback if building javalib's TASTy proves impractical.

Built (step 6): `just javalib-tasty` compiles javalib with sbt (about a minute) into
`ref/libs/javalib-tasty-0.5.12.jar`, 788 files. The frontend opens it with nativelib, clib and
posixlib (javalib's types refer to all three). Beyond the renamed classes above:

- `_Object`'s methods are `__hashCode` and so on; nscplugin strips the `__`.
- Classes nested in a renamed class keep the defined name at run time (`java.lang._String$ASCII$`).
- Members of `@extern` objects are C symbols, not methods.
- `RawPtr` and `RawSize` are primitives (`R_`, `w`).
- windowslib is not loaded: its types stay unresolved.

Oracle: 99.9% of javalib's 10,660 method signatures and all 529 bridges match its NIR.

## 4. The model in Rust [I]

- **Arenas and ids:** `SymbolId`, `TypeId` (hash-consed), `NameId` in arenas owned by a
  `Universe`, like `nir::Pool`. Assign-once fields become `Option`s filled on completion.
- **Laziness without shared mutability:** queries take `&mut Universe` and complete what they
  touch; results are ids, so no borrow outlives a query. tasty-query's `using Context`
  becomes `&mut Universe`.
- **Binders:** lambda and method types refer to their own parameters (`TypeParamRef`,
  `TermParamRef`); in an arena that is an id plus an index, with the binder built first.
- **Cache:** an uncompressed image of the libraries' TASTy with a per-file index, next to the
  NIR library image (ADR 0005), built once per library set.

## 5. Oracles [V]

- **Erased signatures and names:** every public member of scalalib, nativelib and javalib has
  its mangled NIR name in the jars (`nir-dump --index --mangled`). Computing the same name
  from TASTy checks erasure and signature naming end to end, with no JVM. Compiler-generated
  members (`$extension`, `$default$N`, bridges, forwarders) come from lowering, not from TASTy,
  and are excluded.
- **Members, types, linearization, subtyping:** golden dumps from tasty-query run by scala-cli
  (a JVM for the oracle only, as the parser's oracle uses dotty); its test suites (~10k lines)
  also give test cases.
- **Typer results later:** dotc's `-Vprint:typer -Xprint-types`.

## 6. Port order

1. `crates/tasty` from spike 01: names, file sections, AST arena, tag table; the library TASTy
   cache.
2. Package tree and lazy roots; symbols of top-level classes and their members (owners,
   flags, names), bodies skipped.
3. Declared types of members (the type model without subtyping); signed names and erasure;
   oracle: NIR names of scalalib and nativelib.
4. Parents, linearization, member lookup with inherited members; value classes and
   subclass-aware `&`/`|` erasure; oracle: NIR names.
5. Base types, `asSeenFrom`, override matching, bridges (dotty's, and Scala Native's interface
   bridges, which compare erased types); oracle: NIR names. Then subtyping on known types, with
   the interface the typer proposes (research 09).
6. javalib's symbols (§3 B); oracle: javalib's NIR. The overlay's follow with the frontend.
7. Bodies on demand, for `inline`, when the typer needs them.

## 7. Decisions

1. The JDK API source (§3): B, javalib's TASTy (user decision, 2026-09-29).
2. The Scala standard library jar comes from the Coursier cache like the Scala Native jars;
   `just fetch-libs` fetches it too.
