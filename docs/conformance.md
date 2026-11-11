# Conformance

How the VM is checked against Scala Native's own test suites, and where it stands.

## Running

```sh
just fetch-libs                # once: Scala Native jars into ref/libs
scripts/sn-tests.sh all        # compile unit-tests (shared + native, JDK ≤ 17) with the JUnit
                               # plugin into tmp/sn-tests/nir; needs ref/scala-native at v0.5.12
scripts/sn-report.sh           # run them in the VM by group, print the table below
scripts/sn-run.sh Name -Other  # run test classes whose name contains `Name`, except `Other`
just ci-suite                  # all of it from scratch, as CI runs it on main (macOS arm64,
                               # Linux x86-64); the table is the job artifact tmp/sn-report/report.md
```

`tests/junit/runner.scala` is the test runner. The VM gives it the JUnit bootstrapper modules
through the externs in `crates/vm/src/natives/junit.rs`, because Scala Native's reflective
lookup needs metadata that its linker generates. Tests run from the root of `ref/scala-native`,
where their resource files are.

## Results (2026-09-29, macOS arm64 and Linux x86-64 in CI)

| Group | Passed | Failed | Skipped |
|---|---:|---:|---:|
| language (`scala.*`) | 237 | 0 | 1 |
| scala.scalanative | 737 | 1 | 0 |
| java.lang | 400 | 1 | 9 |
| java.io | 247 | 0 | 0 |
| java.net | 107 | 0 | 1 |
| java.math | 780 | 0 | 0 |
| java.nio | 2634 | 0 | 1 |
| java.util | 2651 | 1 | 2 |
| java.util.concurrent | 2934 | 0 | 6 |
| niobuffer | 546 | 0 | 0 |
| posixlib | 61 | 0 | 1 |
| runtime | 2 | 0 | 0 |
| **total** | **11336** | **3** | **21** |

Counts are test methods, from macOS. Linux fails the same tests. It skips four more: the
read-only-file cases, because the CI container runs as root.

With the scalalib jar `just lib-jar scalalib` builds in place of the published one
(`just lib-jar-report scalalib`), every test passes, fails and skips as above. That jar holds
our NIR of the files the frontend lowers (the scalalib row of ROADMAP's Next 5), the published
NIR of the classes Scala Native takes from Scala 2.13, and Scala Native's build's NIR of the
files the frontend refuses. It lacks 91 NIR files of the published jar: anonymous classes and
lambdas that Scala Native's build of Scala 3.9.0 does not emit and no other class references.

A feature the VM lacks throws `java.lang.InternalError("scivolo: unsupported: …")`, a missing class
`NoClassDefFoundError`, a missing C function `UnsatisfiedLinkError`, so one gap fails a test
instead of the run.

## Known gaps behind the failures

- **Service providers.** `ServiceLoader` lists every provider `META-INF/services` names, so
  `ServiceLoaderTest.loadService`'s provider that is not opted in is found too (ADR 0007).
- **Stack traces of threads.** `Thread.getStackTrace` of the current thread lists its frames as
  on the JDK; `ThreadTest.currentThreadGetStackTrace` expects Scala Native's empty array
  (ADR 0008).
- **Object layout.** `IntrinsicsTest.sizeOfClassTest` expects the lock word multithreaded Scala
  Native adds to object headers; the VM keeps monitors outside objects (ADR 0003).
- **Static initializers.** Before `main`, Scala Native runs every `<clinit>` (of `@static`
  values) in the order of its linker's `Reach.done` hash map, which depends on how many
  definitions the program reaches, before the runtime's `init`; the VM runs the `<clinit>`s of
  the files before the library sorted by class name, after `init`, which the heap snapshot
  holds (#188). A dependency jar's file that replaces a library file counts as having a
  `<clinit>` only if the library's has one. Files added to a REPL session run theirs on first
  use of the class (ADR 0047).

## Upstream bugs to report

Found through the VM, not yet reported. Locations are in Scala Native v0.5.12 (`ref/scala-native`).
The VM fixes javalib bugs in its library overlay (`overlay/src`, ADR 0009).

1. **`LinkedTransferQueue.remove` reads `null.next`** (javalib,
   `java/util/concurrent/LinkedTransferQueue.scala:1214`).
   - Port of the JDK loop `for (Node c = p;; q = p.next)`: the update `q = p.next` also runs
     after the `break` that set `p = q = null`.
   - Removing an element the queue does not hold throws `NullPointerException` unless the
     optimizer drops the unused load; `LinkedTransferQueueTest$Generic.testRemoveElement`.
   - Fix: update `q` only when the loop continues. VM: fixed in the overlay.
2. **`ForkJoinPool.helpAsyncBlocker` fails on other threads** (javalib,
   `java/util/concurrent/ForkJoinPool.scala:2063`).
   - `Thread.currentThread() match { case wt: ForkJoinWorkerThread => … }` has no default
     case: `MatchError` where the JDK does nothing.
   - Reached from `SubmissionPublisher` when a producer blocks outside the pool, which green
     threads make likely; 5 `SubmissionPublisherTest` cases.
   - Fix: `case _ => ()`. VM: fixed in the overlay.
3. **`HashSet.containsAll`'s lambda throws** (javalib, `java/util/HashSet.scala:31`).
   - `c.iterator().scalaOps.forall(contains)` on a `Collection[_]` eta-expands `contains` with a
     `Nothing` parameter; the published javalib's lambda body is `throw o`.
   - Fix: `forall(e => contains(e))`. VM: `containsAll` is replaced in the overlay.
4. **A SAM lambda lacks its bridge** (nscplugin's lambda classes; dotty
   `tests/run/i24573.scala`).
   - For `trait ToInt[-T] extends (T => Int) { def apply(t: T): Int }`, the lambda class of
     `(s: String) => s.length` defines `apply(Object): Int` only; a call through `Function1`
     needs `apply(Object): Object`, which the JVM's lambda factory adds as a bridge.
   - The Scala Native binary segfaults; minimal reproduction: `tests/programs/bridges`.
   - Fix: emit the bridges of the functional interface's erased signatures. VM: synthesizes the
     bridge on dispatch (ADR 0008).
5. **No static forwarder beside a `@static` method of the same name** (nscplugin,
   `scala-3/scala/scalanative/nscplugin/NirGenStat.scala:835`).
   - An object with `@static def foo` and an instance method `foo` of the same signature (e.g.
     through `@targetName("foo")`) fails to compile: "found existing public static method …
     cannot generate a static forwarder".
   - Scala 3's JVM backend skips forwarders whose name the companion class already has (dotty
     `tests/run/i19394.scala`).
   - Fix: skip the forwarder instead of reporting an error. VM: not affected.
6. **Static forwarders of inherited methods carry positions in the wrong file** (nscplugin,
   `scala-3/scala/scalanative/nscplugin/NirGenStat.scala:826`).
   - The forwarder's position is the member's span read against the current unit's source
     (`NirPositions.fromSpan`), even when the member is defined in another file.
   - auxlib's `scala.scalanative.runtime.BoxedUnit` forwarders to `equals`/`hashCode`/`toString`
     of `scala/runtime/BoxedUnit.scala` point into `scala/scalanative/runtime/BoxedUnit.scala`,
     past its end.
   - Fix: convert `sym.sourcePos`. VM: positions in the member's own file.
7. **`-language:experimental.X` is named twice in `@experimental`** (dotty,
   `compiler/src/dotty/tools/dotc/typer/Checking.scala:964`).
   - `checkAndAdaptExperimentalImports` writes `-language:experimental.$sel` where `sel` already
     is `experimental.X`: `@experimental("Added by -language:experimental.experimental.erasedDefinitions")`.
   - Fix: `-language:$sel`. Frontend: the same text as dotty.
8. **Stale `.check` files in `tests/run-macros`** (dotty 3.9.0, `tasty-extractors-1`,
   `tasty-extractors-2`, `tasty-extractors-types`, `type-print`).
   - The checks print package prefixes as `ThisType(TypeRef(NoPrefix(), "immutable"))`; 3.9.0
     on Scala Native and on the JVM prints the pickled `TermRef` paths. `tasty-extractors-2` also
     expects `n.toString` in `productElement`, `type-print` other highlight colours.
   - Fix: re-record the checks. VM: compared with `tests/dotty-checks/run-macros`, recorded from
     dotty 3.9.0 on Scala Native.
9. **Anonymous mirrors in inline bodies lack `fromProduct`** (dotty 3.9.0,
   `compiler/src/dotty/tools/dotc/typer/Synthesizer.scala:277`, `anonymousMirror`).
   - `inline def mk(p: Product): Some[Int] = summon[Mirror.ProductOf[Some[Int]]].fromProduct(p)`:
     each inlined copy of the anonymous mirror class calls a `fromProduct` it does not define.
   - Scala Native: "Unknown method `…$anon$1.fromProduct(scala.Product): scala.Some`" at link
     time; JVM: `NoSuchMethodError`.
   - Fix: give the inlined anonymous mirror classes their mirror members. Frontend: defines them.
10. **A trait with an `extension` group has `NoInits` only when read from TASTy** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/ast/TreeInfo.scala:485`, `defKind`).
    - `Namer` takes the flag from `untpd.bodyKind` of the unexpanded body, where `ExtMethods`
      counts as an initializer; `TreeUnpickler.indexStats` sees only methods and sets `NoInits`.
    - A class extending the trait calls its empty `$init$` when compiled with the trait, not
      against its TASTy: nativelib's `unsafe/package$` and `UnsafePackageCompat`.
    - Fix: `defKind` of `ExtMethods` as of its methods. Frontend: as dotty; `just lib-compile
      nativelib` reads `UnsafePackageCompat` from TASTy until #196, so `unsafe/package$` lacks the
      call and `unsafe/package`'s forwarders to its members have the object's position, as in
      separate compilation.
11. **`super[Q].m` of a `@targetName` member crashes `ResolveSuper`** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/transform/ResolveSuper.scala:89`, `decomposeSuperName`).
    - Trait `P extends Q` with `@targetName("tn") def n` calling `P.super[Q].n` from a nested
      class: the accessor's target name `tn$super$Q` is a `SimpleName`, so neither case matches
      (`MatchError`).
    - Fix: decompose the target name's string. Frontend: compiles it, calling `Q`'s `tn`.
12. **`multiSpreads` crash on nested spreads and in inline expansions** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/transform/PostTyper.scala:473,774`, `flattenSpreads`).
    - `f(f(1, xs*), xs*)`: the outer literal is flattened without transforming its elements, so
      the inner `<special-ops>.spread` reaches the backend (`SI-5604` assertion).
    - `inline def g(inline ys: List[Int]) = f(0, ys*, 9)` called as `g(next())`: the lifted
      `ev$N` is owned by the enclosing class, and `LambdaLift` finds no proxy for it.
    - Fix: transform the elements; lift with the owner of the expansion. Frontend: compiles both,
      evaluating each spread once, in order.
13. **Early returns from different stack heights within a `finally` fail verification** (dotty
    3.9.0, `compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala:636`, `genReturn`).
    - `try g2(1, if c == 1 then return 5 else 2) + g3(1, 2, if c == 2 then return 6 else 3)
      finally log()`: both returns jump to the cleanup with the arguments loaded so far still on
      the stack, so its frame has two heights (`VerifyError`).
    - Fix: drop the stack before the jump, as `genAdaptAndSendToDest` does for labels. JVM
      backend: refuses the method.
14. **`break` out of `synchronized` leaves the monitor held** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/transform/DropBreaks.scala`).
    - `boundary { lock.synchronized { if c then break(1); 2 } }`: the break becomes a labeled
      return that skips `monitorexit` (`IllegalMonitorStateException`); across a `try` it stays
      an exception.
    - Fix: keep the exception across `synchronized` as across `try`. Frontend: breaks throw.
15. **`TastyPrinter` reads the payload of a constant as a Nat** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/core/tasty/TastyPrinter.scala:197`, `printTree`).
    - `INTconst`, `LONGconst`, `DOUBLEconst` carry a LongInt; `printNat` fails on a negative or
      large one (`Expected a 31-bit nat`), so a file with `1.0` or a case class's `hashCode`
      seed does not print.
    - Fix: `readLongInt` for those tags. `just jvm-tasty`: compares files without such
      constants only.
16. **`fromProduct` of an F-bounded case class throws** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala`, `fromProductBody`).
    - `case class Node[T <: Ordered[T]](value: T)`: instantiating `T` from above falls back to
      its lower bound, `Nothing`, so `value$1.$asInstanceOf$[Nothing]` throws
      `ClassCastException: Cannot cast to scala.Nothing`.
    - Fix: instantiate it to its bound with itself a wildcard (`Ordered[?]`). VM and JVM: as
      dotty.
17. **`+=` on another object's `@static var` crashes the backend** (dotty 3.9.0).
    - `object Counter { @static var a: Int = 1 }` with `Counter.a += 5` elsewhere: `AssertionError:
      an unexpected type representation reached the compiler backend … NoType`;
      `Counter.a = Counter.a + 5` compiles.
    - JVM: compiles it as the assignment.
18. **A bridge's closure of more than 22 context parameters is a `Function1`** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/transform/Bridges.scala`, `etaExpand`).
    - `def mega: Mega[Int]` overriding an abstract one, `Mega` a context function of 23
      parameters: the bridge returns a closure of the erased `(Object[])` method, a `Function1`
      where callers expect a `FunctionXXL` (`ClassCastException`).
    - Fix: give the closure the type `FunctionXXL`. VM and JVM: as dotty.
19. **Generic signatures flatten one context function result** (dotty 3.9.0,
    `compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala`, `collectMethodParams`).
    - It adds the parameters of one `ContextFunctionN` result to every method's signature, whether
      erasure flattened none (`h(I)Lscala/Function1;` signed `(ILjava/lang/String;)I`) or two.
    - Fix: follow `ContextResultCount`. JVM: as dotty.

## M4: dotty `tests/run`

```sh
just dotty-build    # ref/scala3 at 3.9.0 (sparse), each test compiled to NIR under tmp/dotty
just dotty-report   # run them; report.md and results.tsv (one line per test) under tmp/dotty
just dotty-native   # link the tests failing in the VM as Scala Native binaries, for the report
```

A test passes when `Test.main` exits normally and its stdout matches the `.check` file, if any.
As in dotty, a directory's files are compiled in groups by the `_N` suffix of their names, in
ascending order, each group seeing the output of the ones before it. Skipped: tests for one
platform, with Java sources, compiled partly by another compiler (a `_c3.0.2` suffix), or using
APIs the Scala Native javalib lacks (member reflection, processes, serialization). A VM failure
counts as failing in Scala Native too when the native binary fails the same way with the same
stdout, or does not link for a missing definition. A run with a filter updates only its tests'
rows.

Results (2026-09-29):

| Outcome | Tests |
|---|---:|
| passed | 1504 |
| wrong output | 0 |
| crashed | 0 |
| fails in Scala Native too | 31 |
| compile failed | 1 |
| skipped | 173 |
| **total** | **1709** |

- **Fails in Scala Native too:** reflection Scala Native lacks (`getConstructors`,
  `getEnclosingClass`, `getEnumConstants`) and a `main` returning a value (`t4536`). Tests that
  need `Thread.getStackTrace` or `Runtime.totalMemory` pass in the VM, as do `i24573` (a
  missing lambda bridge) and `redundantParents` (`getInterfaces`) (ADR 0008).
- **Compile failed:** `i19394`, in Scala Native's compiler plugin (upstream bug 5).

### Through the Rust frontend

```sh
just dotty-lower-report   # typer, lowering and VM; lower-report.md and lower-results.tsv under tmp/dotty
```

Each single-file test is lowered by `scivolo-lower` and run in the VM. Tests failing in the VM when
compiled by Scala Native (`dotty-report`) are counted apart; refusals are grouped by the reason
the typer or the lowering gives. The programs `tests/programs/lowered.txt` lists run as
`scivolo FILE.scala` with `SCIVOLO_FRONTEND=strict` (`crates/scivolo/tests/lowered.rs`).

Results (2026-11-11):

| Outcome | Tests |
|---|---:|
| passed | 1496 |
| wrong output | 0 |
| crashed | 0 |
| timeout | 0 |
| lowering refused | 0 |
| typer refused | 10 |
| parser refused | 0 |
| macro refused | 0 |
| frontend crashed | 0 |
| fails in the VM | 32 |
| skipped | 173 |
| **total** | **1711** |

- **Typer refused:** adaptation (2); one each for a local type escaping through a type
  variable, source versions other than 3.7 to 3.9, a context function result of an extension
  method, a context bound of a type without parameters, implicit search with local implicits of
  an inline body, `Printer.TypeReprStructure` of a type the frontend does not print, the
  unresolved name `AnyNamedTuple`, `typeChecks` of a non-constant string. `Runtime.totalMemory`,
  `maxMemory` and `freeMemory`, which Scala Native's javalib lacks and the overlay adds (ADR
  0008), are typed through `libs/jdk-stubs/JdkMemberDeclarations.scala`.
- **`-source future`:** typed as 3.9 with the parser's new-syntax rules; the typer refuses
  alphanumeric infix calls, types and extractor patterns not declared `infix`, which it warns
  about otherwise, and checks no other `future` rule.
- **Capture checking:** typed as dotty's typer types it (`ImpureFunctionN`, `@retains` trees);
  types drop `@retains`, `@retainsCap` and `@retainsByName` as unpickled ones do (case 1: the
  annotations are not refining and erase away), and the capture checker, which runs after the
  typer and leaves the NIR unchanged, is not run (#168). dotty pickles quotes before the capture
  checker, so their TASTy is the typer's: ours as dotty's, with the inferred type arguments of
  inline methods annotated `@caps.internal.inferred` (`PostTyper.markInferred`). Since the
  dropped annotations would be in the quote's inferred types, a quote is refused in a file that
  writes a capture set, an annotation, a function or a by-name type, and a quote that refers to
  a member whose type has a `retains` annotation (e.g. `List(1).view`).

### On the JVM

```sh
just dotty-jvm-report   # scivolo-jvm, then java -Xverify:all; jvm-report.md and jvm-results.tsv under tmp/dotty
just dotty-jvm-report "" run-macros   # tests/run-macros: jvm-report-run-macros.md
just programs-jvm       # tests/programs: programs-report.md and programs-results.tsv under tmp/jvm
just jvm-run FILE       # one file on java -Xverify:all against its .check
just jvm-check FILE     # its class files against scala-cli's, through javap
just jvm-lib-compile scalalib   # Scala's library file by file against dotty's; tmp/lib-jvm/scalalib
```

`scivolo-jvm` writes class version 61 (Java 17) with stack map frames; its output is semantically
equivalent to scalac 3.9.0's, not byte-identical, and scala-cli compiles against the jars it writes.

Each test is compiled by `scivolo-jvm` (#142) and its `Test.main` called through reflection, as
dotty's test runner calls it, on `java -Xverify:all`; its output is compared with dotty's `.check`
file (run-macros: the one recorded in `tests/dotty-checks` where that exists), compiled from the
directory that holds `tests/`. Earlier compilation groups are compiled by scala-cli for the JVM;
for a macro call also for Scala Native, whose NIR the macro runs from in the VM (ADR 0112).
Skipped as in the VM's report, but for APIs Scala Native's javalib lacks. Failures are grouped by
the refusal or the first exception line.

Results (2026-11-09):

| Outcome | tests/run | tests/run-macros |
|---|---:|---:|
| passed | 1594 | 307 |
| wrong output | 0 | 0 |
| verify error | 0 | 0 |
| linkage error | 0 | 0 |
| crashed | 1 | 0 |
| timeout | 0 | 0 |
| jvm refused | 1 | 0 |
| lowering refused | 0 | 0 |
| typer refused | 24 | 7 |
| skipped | 91 | 7 |
| **total** | **1711** | **321** |

- **Typer refused (run):** TASTy pickling (2), members not found (3), JDK annotation classes
  (`Deprecated`, 2), `-Xmixin-force-forwarders:false` (2), adaptation (2), `Printer.TypeReprStructure`
  (`i22649`), the rest singles.
- **Typer refused (run-macros):** `-Xread-docs` (2), a concrete class with an unimplemented member,
  `SuppressWarnings`, adaptation, `Symbol.newClass` with a self type, a source version.
- **JVM refused:** erased parameters of a macro-generated method (`quotes-add-erased`).
- **Crash:** `lambda-serialization-gc` casts the application class loader to `URLClassLoader`,
  which fails for scalac's class files too, as the runner does not load tests through one.
- **Difference from dotty (open, not accepted):** a macro runs from its Scala Native build in the VM, so a
  macro implementation that reads the platform (`System.getProperty`, `scala.util.Properties`,
  runtime class names) sees Scala Native's; what it reflects on (`Symbol`s, types) is the JVM
  build's TASTy, as for dotty.

`just programs-jvm` compiles each of `tests/programs` with `scivolo-jvm`, runs it on
`java -Xverify:all` and compares its stdout and stderr, in one stream, with its `.check` (Scala
Native's output).
Results (2026-11-09):

| Outcome | Programs |
|---|---:|
| passed | 288 |
| wrong output | 5 |
| linkage error | 13 |
| crashed | 3 |
| jvm refused | 2 |
| lowering refused | 1 |
| typer refused | 6 |
| **total** | **318** |

- **As scalac's class files on the JVM** (platform differences): wrong output `bottomarrays`,
  `universaljoins` (array class names), `classtags` (`ClassTag` of a nested array), `staticinit`
  (object initialization order), `unitarrays` (`Array[Unit]` elements); crashes `closuretypes`
  (a package named `java.*`), `matchtypearrays` (`ClassCastException`), `resources` (no resource
  directory on the classpath).
- **Scala Native APIs:** 12 linkage errors (`scala.scalanative.unsafe`, `meta.LinktimeInfo`,
  `runtime.LLVMIntrinsics`), `overflow` (a `scalanative.unsafe` macro call),
  `Ni26intrinsics` (JVM refused: an ordering of references).
- **JVM refused:** `exprforms` (an argument of another JVM type).
- **Linkage error:** `protectedaccess` (`NoSuchMethodError` of `Copies.clone()`: super accessors
  the TASTy lacks).
- **Refused:** TASTy pickling (`mirrorproxies`, `scala2mirrors`, `quotecc`), `dynamic`
  (constructor proxy and an imported term), `hkerasure` (no implicit argument), `Lw26statics`
  (lowering: an `@static` member selected from a value other than a path).

Scala's library (#222, ADR 0110): each of the 649 `.scala` files of `scala-library-3.9.0`'s
sources jar is compiled alone by `scivolo-jvm --scalalib` and its class files compared through
javap text, code and attributes apart, with dotty 3.9.0's from one run over the whole sources jar
with the library build's options without `-opt`. The published jar is no oracle: its build adds
the optimizer (1883 of its 3134 classes dotty also writes differ in code from dotty's) and takes
139 classes and 2.13's specialized ones from Scala 2.13. Results (2026-11-11):

| Outcome | Files | Classes |
|---|---:|---:|
| same | 199 | 1288 |
| attributes differ | 77 | 428 |
| code differs | 243 | 1113 |
| refused | 130 | |

- **Refused:** member not found (96, of them 86 `scala.runtime.java8` traits calling
  `BoxesRunTime`, a Java class of the library's jar); adaptation (9); no applicable alternative
  (8); TASTy pickling (9: a reference to a local definition outside the pickled trees 6, a type
  annotation with arguments 3).
- **Code differs**, by first differing line: a missing `serialVersionUID` field (46), dotty's
  second `athrow` after a call of type `Nothing` (37), generic signatures of trait forwarders and
  extension methods (`compose`, `equals$extension`), field signatures (`underlying`).
- **Attributes differ:** line numbers (163), `private final` flags (30), `Deprecated` (25),
  `InnerClasses` (11).

### com-lihaoyi libraries from source

`just lib-source NAME` compiles each file of a library on its own and compares its NIR with
scala-cli's build of the whole module and, for the files that differ, with scala-cli's build of
that file alone. Differences from both (2026-11-04):

- **Quote type sharing** (fastparse `MacroInlineImpls$`, `MacroRepImpls$`, upickle-implicits
  `macros$package$`; case 1): a quote string writes a type it wrote before as a `SHAREDtype`
  where dotty writes it again, or the reverse. dotty shares types by object identity: a class
  reference read from TASTy (by name), one from a JDK class file (by symbol through the package's
  `this`) and an implicit-scope companion through its class's prefix are different objects;
  ours tells references apart by symbol, name and `this` only. The quote reads back as the same
  trees.

Against the whole-module build alone, sourcecode `Macros$`, mainargs `Macros$` and pprint
`package` differ (case 1: types shared across files, static forwarders of members a package
object inherits); compiled alone, dotty gives ours, mainargs' up to quote positions. upickle's
own 35 files have no scala-cli oracle (Scala 3.9 reports cyclic references in `Api.scala`).

`just lib-compile scalalib` lowers each file of Scala Native's scalalib alone against Scala's
library, its `LazyVals` replaced by Scala Native's (`scripts/scalalib_sources.py`). As in Scala
Native's build of the whole module, a selection of a synthetic member of a case class of Scala's
library (`(a, b).hashCode()`) selects the member it overrides (`Any.hashCode`): those members do
not exist yet while a class of the same run is typed. `ToExpr$ArrayOf*ToExpr$` (8),
`ToExpr$BigIntToExpr$` and `ToExpr$MapToExpr` differ in quote type sharing (case 1, as in
**Quote type sharing** above, types shared across the files of the whole-module build);
scala-cli's build of `ToExpr.scala` alone has the quotes of all 106 classes as ours, up to
positions. Open: `FromExpr$MapFromExpr`'s quotes differ beyond positions, 31 other `FromExpr`
classes in the positions of their quotes; `CNode` calls `RandomGenerator.nextInt(int, int)`
where Scala Native, compiling against the JDK, calls `ThreadLocalRandom`'s override, which
javalib lacks.

## M5: scala/scala `test/junit`

```sh
just m5-build    # ref/scala2 at v2.13.18 (sparse), compiled with Scala 3.9 into tmp/m5
just m5-report   # run each test class; report.md and results.txt under tmp/m5
```

Since Scala 3.8 the standard library is compiled by Scala 3, so scala/scala's JUnit tests for
it (v2.13.18, the version it is based on) are compiled with Scala 3.9 and Scala Native's JUnit
plugin, minus the files Scala Native's denylist for 2.13.18 excludes. Files that do not compile
with Scala 3 are dropped until the rest does; `AssertUtil` gets one Scala 3 fix. Each test
class runs in its own VM, 900 s at most.

Results (2026-09-28): of 104 files, 92 compile; all 867 tests in their 112 classes pass.

- **Not compiling with Scala 3 (12 files):** ambiguous givens (`BuildFromTest`,
  `DecoratorsTest`), missing `ClassTag`s, `==` between unrelated types, and overloads that
  Scala 3 resolves differently (`SortingTest`, `ArraySortingTest`, `RicherTest`).
- **Slowest:** `ListSetTest.hasTailRecursiveDelete` builds a 50,000-element `ListSet`, a
  quadratic 1.25 billion `==` on boxed `Int`s.
