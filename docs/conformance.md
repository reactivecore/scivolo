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

## Results (macOS arm64 and Linux x86-64 in CI)

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
our NIR of all of scalalib's files lowered as one unit, as Scala Native's build compiles them, and
the published NIR of the classes Scala Native takes from Scala 2.13. It lacks 91 NIR files of the published jar: anonymous classes and
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
  use of the class.

## Upstream bugs to report

Found through the VM, not yet reported. Locations are in Scala Native v0.5.12 (`ref/scala-native`).
The VM fixes javalib bugs in its library overlay (`overlay/src`, ADR 0009).

Each **Reproduce** runs in an empty directory holding the files shown. Results are from Scala
3.9.0, Scala Native 0.5.12, scala-cli 1.14.0 and OpenJDK 21 on macOS arm64. Our results come from
the release binaries, run from the repository root:

- JVM: `target/release/scivolo-jvm -d out bug.scala`, then
  `java -Xverify:all -cp out:ref/libs/scala-library-3.9.0.jar bug`.
- VM: `target/release/scivolo bug.scala`.

1. **`LinkedTransferQueue.remove` reads `null.next`** (javalib,
   `java/util/concurrent/LinkedTransferQueue.scala:1214`).
   - Port of the JDK loop `for (Node c = p;; q = p.next)`: the update `q = p.next` also runs
     after the `break` that set `p = q = null`.
   - Removing an element the queue does not hold throws `NullPointerException` unless the
     optimizer drops the unused load; `LinkedTransferQueueTest$Generic.testRemoveElement`.
   - Fix: update `q` only when the loop continues. VM: fixed in the overlay.
   - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 --native --native-version 0.5.12 bug.scala`

     ```scala
     import java.util.concurrent.LinkedTransferQueue

     @main def bug(): Unit =
       val q = new LinkedTransferQueue[String]
       q.add("a")
       println(q.poll())
       println(q.remove("b"))
     ```

   - **Expected**: `a`, then `false`: the queue does not hold `b`. The JVM prints this.
   - **Observed**: Scala Native prints `a`, then fails with `java.lang.NullPointerException` at
     `LinkedTransferQueue.remove`. With `--native-mode release-fast` it prints `a`, `false`. The VM
     prints `a`, `false`.
2. **`ForkJoinPool.helpAsyncBlocker` fails on other threads** (javalib,
   `java/util/concurrent/ForkJoinPool.scala:2063`).
   - `Thread.currentThread() match { case wt: ForkJoinWorkerThread => … }` has no default
     case: `MatchError` where the JDK does nothing.
   - Reached from `SubmissionPublisher` when a producer blocks outside the pool, which green
     threads make likely; 5 `SubmissionPublisherTest` cases.
   - Fix: `case _ => ()`. VM: fixed in the overlay.
   - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 --native --native-version 0.5.12 bug.scala`

     ```scala
     import java.util.concurrent.{Flow, ForkJoinPool, SubmissionPublisher, TimeUnit}

     @main def bug(): Unit =
       val pub = new SubmissionPublisher[Integer](new ForkJoinPool(1), 1)
       pub.subscribe(new Flow.Subscriber[Integer]:
         def onSubscribe(s: Flow.Subscription): Unit = ()
         def onNext(i: Integer): Unit = ()
         def onError(t: Throwable): Unit = ()
         def onComplete(): Unit = ())
       for i <- 1 to 3 do
         println(pub.offer(Integer.valueOf(i), 100, TimeUnit.MILLISECONDS, null))
       pub.close()
     ```

   - **Expected**: `1`, `-1`, `-1`: the subscriber requests nothing, so later offers time out and
     are dropped. The JVM prints this.
   - **Observed**: Scala Native prints `1`, then fails with `scala.MatchError:
     Thread[#0,main,5,main] (of class java.lang.Thread$MainThread$)` at
     `ForkJoinPool$.helpAsyncBlocker`. The VM prints `1`, `-1`, `-1`.
3. **`HashSet.containsAll`'s lambda throws** (javalib, `java/util/HashSet.scala:31`).
   - `c.iterator().scalaOps.forall(contains)` on a `Collection[_]` eta-expands `contains` with a
     `Nothing` parameter; the published javalib's lambda body is `throw o`.
   - Fix: `forall(e => contains(e))`. VM: `containsAll` is replaced in the overlay.
   - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 --native --native-version 0.5.12 bug.scala`

     ```scala
     import java.util.{Arrays, HashSet}

     @main def bug(): Unit =
       val s = new HashSet[String]
       s.add("a")
       println(s.containsAll(Arrays.asList("a")))
     ```

   - **Expected**: `true`, as on the JVM.
   - **Observed**: Scala Native fails with `scala.scalanative.runtime.UndefinedBehaviorError` at
     `java.util.HashSet.containsAll$$anonfun$1`. The VM prints `true`.
4. **A SAM lambda lacks its bridge** (nscplugin's lambda classes; dotty
   `tests/run/i24573.scala`).
   - For `trait ToInt[-T] extends (T => Int) { def apply(t: T): Int }`, the lambda class of
     `(s: String) => s.length` defines `apply(Object): Int` only; a call through `Function1`
     needs `apply(Object): Object`, which the JVM's lambda factory adds as a bridge.
   - The Scala Native binary segfaults; minimal reproduction: `tests/programs/bridges`.
   - Fix: emit the bridges of the functional interface's erased signatures. VM: synthesizes the
     bridge on dispatch (ADR 0008).
   - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 --native --native-version 0.5.12 bug.scala`

     ```scala
     trait ToInt[-T] extends (T => Int):
       def apply(t: T): Int

     def viaFunction[A, B](f: A => B, a: A): B = f(a)

     @main def bug(): Unit =
       val length: ToInt[String] = (s: String) => s.length
       println(viaFunction(length, "four"))
     ```

   - **Expected**: `4`, as on the JVM.
   - **Observed**: the Scala Native binary stops with `ScalaNative: Unhandled signal 11,
     si_addr=0x0` in `viaFunction`. Replacing the last two lines of `bug` by
     `val f: ToInt[String] = (s: String) => s.length`, `val g: String => Int = f` and
     `println(g("abc"))` fails at link time instead: `Unknown method
     bug$package$$$Lambda$1.apply(java.lang.Object): java.lang.Object`. The VM prints `4`.
5. **No static forwarder beside a `@static` method of the same name** (nscplugin,
   `scala-3/scala/scalanative/nscplugin/NirGenStat.scala:835`).
   - An object with `@static def foo` and an instance method `foo` of the same signature (e.g.
     through `@targetName("foo")`) fails to compile: "found existing public static method …
     cannot generate a static forwarder".
   - Scala 3's JVM backend skips forwarders whose name the companion class already has (dotty
     `tests/run/i19394.scala`).
   - Fix: skip the forwarder instead of reporting an error. VM and JVM: skip it, as dotty does
     (`tests/jvm/staticforwarders`).
   - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 --native --native-version 0.5.12 bug.scala`

     ```scala
     import scala.annotation.{static, targetName}

     class Foo
     object Foo:
       @static def foo: String = "foo"
       @targetName("foo") def fooBincompat: String = foo

     @main def bug(): Unit = println(Foo.foo)
     ```

   - **Expected**: `foo`, as on the JVM.
   - **Observed**: Scala Native reports at `class Foo`: `Unexpected situation: found existing public
     static method method fooBincompat in the companion class of Foo$; cannot generate a static
     forwarder the method of the same name in the object.` The VM and `scivolo-jvm` print `foo`.
6. **Static forwarders of inherited methods carry positions in the wrong file** (nscplugin,
   `scala-3/scala/scalanative/nscplugin/NirGenStat.scala:826`).
   - The forwarder's position is the member's span read against the current unit's source
     (`NirPositions.fromSpan`), even when the member is defined in another file.
   - auxlib's `scala.scalanative.runtime.BoxedUnit` forwarders to `equals`/`hashCode`/`toString`
     of `scala/runtime/BoxedUnit.scala` point into `scala/scalanative/runtime/BoxedUnit.scala`,
     past its end.
   - Fix: convert `sym.sourcePos`. Lowering: the same position as Scala Native.
   - **Reproduce**: with `A.scala` and `O.scala` below,
     `scala-cli compile --server=false --scala 3.9.0 --native --native-version 0.5.12 -d out A.scala O.scala`,
     then `target/release/scivolo-nir-dump --debug out/O.nir`.

     ```scala
     // A comment that makes the offset of the member below larger than the size of O.scala.
     class A:
       def inherited: Int = 1
     ```

     ```scala
     class O
     object O extends A
     ```

   - **Expected**: the forwarder `O::static inherited` has a position inside a source file, that of
     `inherited` in `A.scala`.
   - **Observed**: `@ O.scala:2:76`, zero-based, while `O.scala` has two lines. In the published
     `auxlib_native0.5_3-0.5.12.jar`, the forwarders of `scala/scalanative/runtime/BoxedUnit.nir`
     are at `BoxedUnit.scala:4:71` (`hashCode`), `3:42` (`equals`) and `4:106` (`toString`); that
     file has four lines. `target/release/scivolo-lower A.scala O.scala low` also gives
     `@ O.scala:2:76`.
7. **`-language:experimental.X` is named twice in `@experimental`** (dotty,
   `compiler/src/dotty/tools/dotc/typer/Checking.scala:964`).
   - `checkAndAdaptExperimentalImports` writes `-language:experimental.$sel` where `sel` already
     is `experimental.X`: `@experimental("Added by -language:experimental.experimental.erasedDefinitions")`.
   - Fix: `-language:$sel`. Frontend: the same text as dotty.
   - **Reproduce**: `scala-cli compile --server=false --scala 3.9.0 -d out bug.scala`, then
     `strings out/C.tasty | grep 'Added by'`.

     ```scala
     //> using options -language:experimental.erasedDefinitions

     class C
     ```

   - **Expected**: `Added by -language:experimental.erasedDefinitions`, the option as written.
   - **Observed**: `Added by -language:experimental.experimental.erasedDefinitions`, from dotty
     and from `scivolo-jvm`.
8. **Stale `.check` files in `tests/run-macros`** (dotty 3.9.0, `tasty-extractors-1`,
   `tasty-extractors-2`, `tasty-extractors-types`, `type-print`).
   - The checks print package prefixes as `ThisType(TypeRef(NoPrefix(), "immutable"))`; 3.9.0
     on Scala Native and on the JVM prints the pickled `TermRef` paths. `tasty-extractors-2` also
     expects `n.toString` in `productElement`, `type-print` other highlight colours.
   - Fix: re-record the checks. VM: compared with `tests/dotty-checks/run-macros`, recorded from
     dotty 3.9.0 on Scala Native.
   - **Reproduce**: the named tests of `ref/scala3/tests/run-macros`, on the JVM, the macro file
     compiled first as dotty's test framework does. In `tasty-extractors-1` (the others alike;
     `type-print` has `Macro_1.scala` and `Test_2.scala`):

     ```sh
     scala-cli compile --server=false --scala 3.9.0 --with-compiler -d m quoted_1.scala
     scala-cli compile --server=false --scala 3.9.0 --with-compiler --classpath m -d t quoted_2.scala
     scala-cli run --server=false --scala 3.9.0 --with-compiler --classpath m:t --main-class Test > out
     diff out ../tasty-extractors-1.check
     ```

   - **Expected**: no difference.
   - **Observed**: all four exit with 0 and differ from their checks, in 3 lines
     (`tasty-extractors-1`), 2 (`tasty-extractors-2`), 3 (`tasty-extractors-types`) and 4
     (`type-print`). `tasty-extractors-1` prints
     `TypeRef(TermRef(ThisType(TypeRef(NoPrefix(), "<root>")), "scala"), "Unit")` where the check has
     `TypeRef(ThisType(TypeRef(NoPrefix(), "scala")), "Unit")`. `tasty-extractors-2` prints
     `List(Ident("n"))` where the check has `List(Apply(Select(Ident("n"), "toString"), Nil))`.
     `type-print` colours `scala.collection.immutable.List` differently. Not run on Scala Native.
9. **Anonymous mirrors in inline bodies lack `fromProduct`** (dotty 3.9.0,
   `compiler/src/dotty/tools/dotc/typer/Synthesizer.scala:277`, `anonymousMirror`).
   - `inline def mk(p: Product): Some[Int] = summon[Mirror.ProductOf[Some[Int]]].fromProduct(p)`:
     each inlined copy of the anonymous mirror class calls a `fromProduct` it does not define.
   - Scala Native: "Unknown method `…$anon$1.fromProduct(scala.Product): scala.Some`" at link
     time; JVM: `NoSuchMethodError`.
   - Fix: give the inlined anonymous mirror classes their mirror members. VM and JVM: the copies
     keep the members our PostTyper step gave the inline body (`tests/jvm/inlinemirrors`).
   - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`, and with
     `--native --native-version 0.5.12` for Scala Native.

     ```scala
     import scala.deriving.Mirror

     inline def mk(p: Product): Some[Int] = summon[Mirror.ProductOf[Some[Int]]].fromProduct(p)

     @main def bug(): Unit = println(mk(Tuple1(1)))
     ```

   - **Expected**: `Some(1)`: the mirror builds a `Some` from the product.
   - **Observed**: the JVM fails with `java.lang.NoSuchMethodError: 'scala.Some
     bug$package$$anon$1.fromProduct(scala.Product)'`; Scala Native at link time with `Unknown method
     bug$package$$anon$1.fromProduct(scala.Product): scala.Some`. The VM and `scivolo-jvm` print
     `Some(1)`.
10. **A trait with an `extension` group has `NoInits` only when read from TASTy** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/ast/TreeInfo.scala:485`, `defKind`).
    - `Namer` takes the flag from `untpd.bodyKind` of the unexpanded body, where `ExtMethods`
      counts as an initializer; `TreeUnpickler.indexStats` sees only methods and sets `NoInits`.
    - A class extending the trait calls its empty `$init$` when compiled with the trait, not
      against its TASTy: nativelib's `unsafe/package$` and `UnsafePackageCompat`.
    - Fix: `defKind` of `ExtMethods` as of its methods. Frontend: as dotty; `just lib-compile
      nativelib` lowers `UnsafePackageCompat` with `unsafe/package.scala` as one unit, as Scala
      Native's joint build does.
    - **Reproduce**: with `T.scala` and `C.scala` below:

      ```scala
      trait T:
        extension (i: Int) def twice: Int = i * 2
      ```

      ```scala
      class C extends T
      ```

      ```sh
      scala-cli compile --server=false --scala 3.9.0 -d joint T.scala C.scala
      scala-cli compile --server=false --scala 3.9.0 -d t T.scala
      scala-cli compile --server=false --scala 3.9.0 --classpath t -d sep C.scala
      javap -c -p -cp joint C | grep invoke
      javap -c -p -cp sep:t C | grep invoke
      ```

    - **Expected**: the same constructor of `C` in both runs: compiling `T` with `C` or reading it
      from TASTy does not change what `C` does.
    - **Observed**: the joint run's constructor has `invokestatic InterfaceMethod T.$init$:(LT;)V`;
      the separate run's has no such call. `scivolo-jvm` gives the same two results.
11. **`super[Q].m` of a `@targetName` member crashes `ResolveSuper`** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/transform/ResolveSuper.scala:89`, `decomposeSuperName`).
    - Trait `P extends Q` with `@targetName("tn") def n` calling `P.super[Q].n` from a nested
      class: the accessor's target name `tn$super$Q` is a `SimpleName`, so neither case matches
      (`MatchError`).
    - Fix: decompose the target name's string. Frontend: compiles it, calling `Q`'s `tn`.
    - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`

      ```scala
      import scala.annotation.targetName

      trait Q:
        @targetName("tn") def n: Int = 1

      trait P extends Q:
        @targetName("tn") override def n: Int = 2
        class Inner:
          def viaSuper: Int = P.super[Q].n

      class C extends P

      @main def bug(): Unit =
        val c = new C
        println((new c.Inner).viaSuper)
      ```

    - **Expected**: it compiles and prints `1`, the result of `Q`'s `n`.
    - **Observed**: dotty crashes in `resolveSuper`'s phase with `scala.MatchError: tn$super$Q (of
      class dotty.tools.dotc.core.Names$SimpleName)`. `scivolo-jvm`'s program prints `1`.
12. **`multiSpreads` crash on nested spreads and in inline expansions** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/transform/PostTyper.scala:473,774`, `flattenSpreads`).
    - `f(f(1, xs*), xs*)`: the outer literal is flattened without transforming its elements, so
      the inner `<special-ops>.spread` reaches the backend (`SI-5604` assertion).
    - `inline def g(inline ys: List[Int]) = f(0, ys*, 9)` called as `g(next())`: the lifted
      `ev$N` is owned by the enclosing class, and `LambdaLift` finds no proxy for it.
    - Fix: transform the elements; lift with the owner of the expansion. Frontend: compiles both,
      evaluating each spread once, in order.
    - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`, for each program:

      ```scala
      import scala.language.experimental.multiSpreads

      def f(xs: Int*): Int = xs.sum

      @main def bug(): Unit =
        val xs = List(1, 2)
        println(f(f(1, xs*), xs*))
      ```

      ```scala
      import scala.language.experimental.multiSpreads

      def f(xs: Int*): Int = xs.sum

      inline def g(inline ys: List[Int]): Int = f(0, ys*, 9)

      var calls = 0
      def next(): List[Int] =
        calls += 1
        List(calls)

      @main def bug(): Unit =
        println(g(next()))
        println(calls)
      ```

    - **Expected**: the first prints `7`; the second `10`, then `1`.
    - **Observed**: dotty crashes on both. First program: `java.lang.AssertionError: SI-5604: Cannot
      use package as value: This(Ident(<special-ops>))`. Second program:
      `java.lang.IllegalArgumentException: Could not find proxy for private val ev$2:
      scala.collection.immutable.List`, in `lambdaLift`'s phase. `scivolo-jvm`'s programs print `7`
      and `10`, `1`.
13. **Early returns from different stack heights within a `finally` fail verification** (dotty
    3.9.0, `compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala:636`, `genReturn`).
    - `try g2(1, if c == 1 then return 5 else 2) + g3(1, 2, if c == 2 then return 6 else 3)
      finally log()`: both returns jump to the cleanup with the arguments loaded so far still on
      the stack, so its frame has two heights (`VerifyError`).
    - Fix: drop the stack before the jump, as `genAdaptAndSendToDest` does for labels. JVM
      backend: refuses the method.
    - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`

      ```scala
      def log(): Unit = println("finally")
      def g2(a: Int, b: Int): Int = a + b
      def g3(a: Int, b: Int, c: Int): Int = a + b + c

      def m(c: Int): Int =
        try g2(1, if c == 1 then return 5 else 2) + g3(1, 2, if c == 2 then return 6 else 3)
        finally log()

      @main def bug(): Unit =
        println(m(0))
        println(m(1))
        println(m(2))
      ```

    - **Expected**: `finally`, `9`, `finally`, `5`, `finally`, `6`.
    - **Observed**: `java.lang.VerifyError: Inconsistent stackmap frames at branch target 45`, at
      `bug$package$.m(I)I @9: goto`, `Current frame's stack size doesn't match stackmap`.
      `scivolo-jvm` refuses it: `the JVM backend does not support method m: stack heights differ
      where control flow joins`.
14. **`break` out of `synchronized` leaves the monitor held** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/transform/DropBreaks.scala`).
    - `boundary { lock.synchronized { if c then break(1); 2 } }`: the break becomes a labeled
      return that skips `monitorexit` (`IllegalMonitorStateException`); across a `try` it stays
      an exception.
    - Fix: keep the exception across `synchronized` as across `try`. Frontend: breaks throw.
    - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`

      ```scala
      import scala.util.boundary
      import scala.util.boundary.break

      val lock = new Object

      def f(c: Boolean): Int =
        boundary:
          lock.synchronized:
            if c then break(1)
            2

      @main def bug(): Unit =
        println(f(false))
        println(f(true))
        println(Thread.holdsLock(lock))
      ```

    - **Expected**: `2`, `1`, `false`: leaving `synchronized` releases the monitor.
    - **Observed**: `2`, then `java.lang.IllegalMonitorStateException` at `bug$package$.f`.
      `scivolo-jvm`'s program prints `2`, `1`, `false`.
15. **`TastyPrinter` reads the payload of a constant as a Nat** (dotty 3.9.0 and 3.10.1,
    `compiler/src/dotty/tools/dotc/core/tasty/TastyPrinter.scala:197`, `printTree`).
    - The numeric constants but `CHARconst` carry a signed Int or LongInt; `printNat` prints a negative one wrong and
      fails on one above 31 bits as a nat (`Expected a 31-bit nat`), so a file with `1.0` or a
      case class's `hashCode` seed `0xcafebabe` (`case class P(a: Int, b: String)`) does not print.
    - Fix: `readLongInt` for those tags. `just jvm-tasty`: prints both sides with these constants
      cut to four bytes and compares the cut constants in full.
    - **Reproduce**: `scala-cli compile --server=false --scala 3.9.0 -d out bug.scala`, then for
      `V` in `3.9.0`, `3.10.1` and `F` in `P`, `Constants`, `Negative`:
      `coursier launch org.scala-lang:scala3-compiler_3:V -M
      dotty.tools.dotc.core.tasty.TastyPrinter -- out/F.tasty`.

      ```scala
      case class P(a: Int, b: String)

      object Constants:
        val d = 1.0

      object Negative:
        val i = -1
      ```

    - **Expected**: each file prints, with the constants' values (`INTconst -1` for `Negative`).
    - **Observed**, the same on 3.9.0 and 3.10.1: `P` fails with
      `dotty.tools.tasty.UnpickleException: Expected a 31-bit nat, got: 33470462654`, `Constants`
      with `Expected a 31-bit nat, got: 4607182418800017408`, and `Negative` prints `INTconst 127`.
      `just jvm-tasty bug.scala` prints `Constants.tasty: same`, `Negative.tasty: same`,
      `P.tasty: same`.
16. **`fromProduct` of an F-bounded case class throws** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala`, `fromProductBody`).
    - `case class Node[T <: Ordered[T]](value: T)`: instantiating `T` from above falls back to
      its lower bound, `Nothing`, so `value$1.$asInstanceOf$[Nothing]` throws
      `ClassCastException: Cannot cast to scala.Nothing`.
    - Fix: instantiate it to its bound with itself a wildcard (`Ordered[?]`). VM and JVM: as
      dotty.
    - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`

      ```scala
      import scala.deriving.Mirror

      case class V(i: Int) extends Ordered[V]:
        def compare(that: V): Int = i - that.i

      case class Node[T <: Ordered[T]](value: T)

      @main def bug(): Unit =
        println(summon[Mirror.ProductOf[Node[V]]].fromProduct(Tuple1(V(1))))
      ```

    - **Expected**: `Node(V(1))`.
    - **Observed**: `java.lang.ClassCastException: Cannot cast to scala.Nothing`, also from
      `scivolo-jvm`'s program and on the VM.
17. **`+=` on another object's `@static var` crashes the backend** (dotty 3.9.0).
    - `object Counter { @static var a: Int = 1 }` with `Counter.a += 5` elsewhere: `AssertionError:
      an unexpected type representation reached the compiler backend … NoType`;
      `Counter.a = Counter.a + 5` compiles.
    - VM and JVM: `{ qual; a = rhs }` for `qual.a = rhs` (`tests/jvm/staticassign`).
    - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`

      ```scala
      import scala.annotation.static

      class Counter
      object Counter:
        @static var a: Int = 1

      @main def bug(): Unit =
        Counter.a += 5
        println(Counter.a)
      ```

    - **Expected**: `6`, what the program prints with `Counter.a = Counter.a + 5`.
    - **Observed**: dotty fails with `java.lang.AssertionError: an unexpected type representation
      reached the compiler backend while compiling bug.scala: NoType.` With the assignment it
      prints `6`. The VM and `scivolo-jvm` print `6`.
18. **A bridge's closure of more than 22 context parameters is a `Function1`** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/transform/Bridges.scala`, `etaExpand`).
    - `def mega: Mega[Int]` overriding an abstract one, `Mega` a context function of 23
      parameters: the bridge returns a closure of the erased `(Object[])` method, a `Function1`
      where callers expect a `FunctionXXL` (`ClassCastException`).
    - Fix: give the closure the type `FunctionXXL`. VM and JVM: do so (`tests/jvm/contextxxl`).
    - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`

      ```scala
      type I = Int
      type Mega[R] = (I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I) ?=> R

      trait A[T]:
        def mega: Mega[T]

      class B extends A[Int]:
        def mega: Mega[Int] = 42

      @main def bug(): Unit =
        given Int = 1
        val a: A[Int] = new B
        println(a.mega)
      ```

    - **Expected**: `42`.
    - **Observed**: `java.lang.ClassCastException: class B$$Lambda/0x0000008801093f90 cannot be cast
      to class scala.runtime.FunctionXXL`. The VM and `scivolo-jvm` print `42`.
19. **Generic signatures flatten one context function result** (dotty 3.9.0,
    `compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala`, `collectMethodParams`).
    - It adds the parameters of one `ContextFunctionN` result to every method's signature, whether
      erasure flattened none (`h(I)Lscala/Function1;` signed `(ILjava/lang/String;)I`) or two.
    - Fix: follow `ContextResultCount`. JVM: as dotty.
    - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`

      ```scala
      abstract class Sigs:
        def twoResults(i: Int): String ?=> Long ?=> Int = summon[String].length + i
        def deferred(i: Int): String ?=> Int

      @main def bug(): Unit =
        for m <- classOf[Sigs].getDeclaredMethods.sortBy(_.getName) do
          println(m.toString + "\n  generic: " + m.toGenericString)
      ```

    - **Expected**: each generic signature has the parameters and result of its descriptor.
    - **Observed**, from dotty and `scivolo-jvm`'s program alike:

      ```text
      public abstract scala.Function1 Sigs.deferred(int)
        generic: public abstract int Sigs.deferred(int,java.lang.String)
      public int Sigs.twoResults(int,java.lang.String,long)
        generic: public scala.Function1<java.lang.Object, java.lang.Object> Sigs.twoResults(int,java.lang.String)
      ```
20. **A shadowed class type parameter named through an alias keeps its name** (dotty 3.9.0,
    `compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala:582`, `collectUsedTypeParams`).
    - `class Sigs[A] { type Elem = A; def m[A](a: A, e: Elem): A = a }`: `foreachPart` does not
      dealias `Elem`, so the method's `A` is not renamed, and `e`, signed after dealiasing, names
      the method's `A`: `<A:Ljava/lang/Object;>(TA;TA;)TA;`.
    - Fix: dealias the parts. JVM: `<A1:Ljava/lang/Object;>(TA1;TA;)TA1;`.
    - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`

      ```scala
      class Sigs[A]:
        type Elem = A
        def m[A](a: A, e: Elem): A = a

      @main def bug(): Unit =
        val m = classOf[Sigs[?]].getDeclaredMethods.find(_.getName == "m").get
        println(m.toGenericString)
        println(m.getTypeParameters.toList)
        println(m.getGenericParameterTypes.toList)
      ```

    - **Expected**: `e` names the class's `A`, not the method's, so the method's parameter is
      renamed: `public <A1> A1 Sigs.m(A1,A)`.
    - **Observed**: `public <A> A Sigs.m(A,A)`, `List(A)`, `List(A, A)`. `scivolo-jvm`'s program
      prints `public <A1> A1 Sigs.m(A1,A)`, `List(A1)`, `List(A1, A)`.
21. **A generic signature's `void` result disagrees with the descriptor** (dotty 3.9.0,
    `compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala:189`, `methodResultSig`).
    - It writes `V` when the result's `typeSymbol` is `Unit`, which follows method type parameter
      bounds but not aliases: `def f[B <: Unit](b: B): B` (descriptor result `BoxedUnit`) is signed
      `…V`, `type U = Unit; def g(x: List[Int]): U` (descriptor result `V`) `…BoxedUnit;`.
    - A Java caller compiled against either links the signature's erasure: `NoSuchMethodError`.
    - Fix: `V` exactly when the descriptor's result is `V`. JVM: does so.
    - **Reproduce**: with `bug.scala` and `Caller.java` below, first
      `scala-cli run --server=false --scala 3.9.0 bug.scala`, then, with the jar of `ref/libs`:

      ```scala
      class S:
        type U = Unit
        def f[B <: Unit](b: B): B = b
        def g(x: List[Int]): U = ()

      @main def bug(): Unit =
        for m <- classOf[S].getDeclaredMethods.sortBy(_.getName) do
          println(m.getName + ": erased " + m.getReturnType.getName + ", generic " + m.getGenericReturnType.getTypeName)
      ```

      ```java
      public class Caller {
        public static void main(String[] args) {
          S s = new S();
          try {
            s.f(scala.runtime.BoxedUnit.UNIT);
            System.out.println("f ok");
          } catch (NoSuchMethodError e) {
            System.out.println(e);
          }
          try {
            s.g(null);
            System.out.println("g ok");
          } catch (NoSuchMethodError e) {
            System.out.println(e);
          }
        }
      }
      ```

      ```sh
      scala-cli compile --server=false --scala 3.9.0 -d out bug.scala
      javac -cp out:scala-library-3.9.0.jar -d j Caller.java
      java -cp j:out:scala-library-3.9.0.jar Caller
      ```

    - **Expected**: each signature's result erases to the descriptor's, so `Caller` prints `f ok`,
      `g ok`.
    - **Observed**: `bug` prints `f: erased scala.runtime.BoxedUnit, generic void` and `g: erased
      void, generic scala.runtime.BoxedUnit`. `Caller` prints `java.lang.NoSuchMethodError: 'void
      S.f(scala.runtime.BoxedUnit)'` and `java.lang.NoSuchMethodError: 'scala.runtime.BoxedUnit
      S.g(scala.collection.immutable.List)'`. With `scivolo-jvm`'s classes, `bug` prints `f: erased
      scala.runtime.BoxedUnit, generic B` and `g: erased void, generic void`, `Caller` `f ok`, `g ok`.
22. **A pattern-bound type constructor applied keeps its bounds as supertype** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/core/Types.scala:4742`, `AppliedType.superType`).
    - `def t1[Exp[_, _], T](x: L[Exp, T]): T = x match { case ap: Ap[e, a, b] => … }` where
      `Ap[E[_, _], A, B] extends L[E, E[A, B]]`: the pattern's GADT inference computes the cached
      supertype of `e[a, b]` as `Any` (`e <: [_, _] =>> Any`); `indexPattern` then makes `e` an
      alias of `Exp`, but the cache is valid for the whole period.
    - `val z: e[a, b] = 42; z` compiles (`Int <: Any`, the cached supertype), and
      `t1[Function1, Int => String](Ap())(3)` throws `ClassCastException`; `e[b, a]`, not cached,
      is refused.
    - `val f: Exp[a, b] = ap.x` (`ap.x: e[a, b]`) gets a GADT cast to `ap.x & Exp[a, b]`, as
      `c.first` in `gadt-TypeSafeLambda`; erasure drops it.
    - Fix: mark the supertype of an application of a pattern-bound type provisional. Frontend:
      the supertype from the current info: refuses `z`, no cast.
    - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`; the second program,
      showing the cast, with `scala-cli compile --server=false --scala 3.9.0 bug.scala`.

      ```scala
      sealed trait L[E[_, _], T]
      case class Ap[E[_, _], A, B]() extends L[E, E[A, B]]

      def t1[Exp[_, _], T](x: L[Exp, T]): T = x match
        case ap: Ap[e, a, b] =>
          val z: e[a, b] = 42
          z

      @main def bug(): Unit =
        println(t1[Function1, Int => String](Ap())(3))
      ```

      ```scala
      //> using options -Vprint:typer

      sealed trait L[E[_, _], T]
      case class Ap[E[_, _], A, B](x: E[A, B]) extends L[E, E[A, B]]

      def t1[Exp[_, _], T](x: L[Exp, T]): Any = x match
        case ap: Ap[e, a, b] =>
          val f: Exp[a, b] = ap.x
          f
      ```

    - **Expected**: the first program is refused: `e` is an alias of `Exp` there, and `42` is no
      `Exp[a, b]`, as dotty itself reports for `e[b, a]`.
    - **Observed**: the first program compiles and fails at run time with
      `java.lang.ClassCastException: class java.lang.Integer cannot be cast to class
      scala.Function1`. With `val z: e[b, a] = 42` and `???` in place of `z`, dotty reports `Found:
      (42 : Int)`, `Required: e[b, a]`. The second program prints `val f: Exp[a, b] =
      ap.x.$asInstanceOf[((ap.x : e[a, b]) & Any & Exp[a, b]) @uncheckedStable]`. `scivolo-jvm`
      refuses `val z` in both forms (`the typer does not support adaptation`) and compiles the
      second program without its `using` line.
23. **A negated character literal of a Java source is a positive constant** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/parsing/JavaParsers.scala:1094`, `tryConstant`).
    - `CHARLIT` ignores the `-` read before it: `static final int N = -'a';` in a Java source of
      the run gets type `97` while javac stores `-97`.
    - A Scala `J.N` is inlined as `97`.
    - Fix: no constant for a negated character. Frontend: does so, `J.N` reads the field.
    - **Reproduce**: with `J.java` and `bug.scala` below, `scala-cli run --scala 3.9.0 .`, which
      uses the build server: it compiles the Java file too.

      ```java
      public class J {
        public static final int N = -'a';
      }
      ```

      ```scala
      @main def bug(): Unit =
        println(J.N)
        println(classOf[J].getField("N").get(null))
      ```

    - **Expected**: `-97` twice: javac stores `-97`.
    - **Observed**: `97`, then `-97`. `target/release/scivolo-jvm -d out bug.scala J.java` gives a
      program printing `-97` twice.
24. **A default getter of a repeated method re-evaluates the prefix** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/typer/Applications.scala:589`, `needLiftFun`).
    - `needLiftFun` counts the repeated argument as one for a fixed parameter, so
      `obj().h(xs = "o", a = 2)` with `def h(a: Int = 7, b: String = "b", xs: String*)` types
      `obj().h$default$2` before the reordering lifts `obj()`: `obj()` runs twice, where SLS
      6.6.1 evaluates the prefix once.
    - Fix: count the fixed parameters' arguments only. Frontend: evaluates `obj()` once.
    - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`

      ```scala
      class O:
        def h(a: Int = 7, b: String = "b", xs: String*): String = s"$a $b ${xs.mkString(",")}"

      def obj(): O =
        println("obj()")
        new O

      @main def bug(): Unit =
        println(obj().h(xs = "o", a = 2))
      ```

    - **Expected**: `obj()` once, then `2 b o` (SLS 6.6.1).
    - **Observed**: `obj()`, `obj()`, `2 b o`. `scivolo-jvm`'s program prints `obj()`, `2 b o`.
25. **A named array spread beside a default argument throws `ClassCastException`** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/typer/EtaExpansion.scala`, `LiftComplex`).
    - `k(xs = Array("x", "y")*)` with `def k(n: Int = 1, xs: String*)` lifts the argument to
      `val xs$2: String* = Array(...)*`; the `val` erases to `Seq` and holds the array, so the
      program fails at run time. A `Seq` spread works.
    - Fix: wrap the array when lifting. Frontend: prints `1 x,y`
      (`tests/jvm/namedarrayspread`).
    - **Reproduce**: `scala-cli run --server=false --scala 3.9.0 bug.scala`

      ```scala
      def k(n: Int = 1, xs: String*): String = s"$n ${xs.mkString(",")}"

      @main def bug(): Unit =
        println(k(xs = Array("x", "y")*))
      ```

    - **Expected**: `1 x,y`, as with a `Seq` spread.
    - **Observed**: `java.lang.ClassCastException: class [Ljava.lang.String; cannot be cast to class
      scala.collection.immutable.Seq`. `scivolo-jvm`'s program prints `1 x,y`.

## Own frontend: parser, symbols, typer

- **Parser** (`just syntax-report`): tokens match dotty on all 5,893 files, trees and
  desugaring on 5,891.
- **Symbols from TASTy:** 99.8% of the library's signatures match Scala Native's mangled names.
- **Typer** (`just typer-report`): each file typed by dotty's oracle and ours,
  compared node by node, against the JDK; `neg` counts files whose errors match; tests/programs
  also against Scala Native's javalib.

| Outcome | tests/programs | tests/pos | tests/neg | tests/programs (Native) |
|---|---:|---:|---:|---:|
| passed | 287 | 2702 | 1300 | 283 |
| differ | 24 | 85 | 0 | 28 |
| warnings differ | 0 | 9 | 0 | 0 |
| accepted (neg) | 0 | 0 | 2 | 0 |
| unsupported | 1 | 242 | 0 | 1 |
| crashed | 0 | 0 | 0 | 0 |
| skipped | 32 | 338 | 492 | 0 |

Refusals and differences of the frontend and the lowering:

- **GADT patterns** (ADR 0119). Refused:
  - pattern alternatives that narrow differently (`different GADT bounds from pattern
    alternatives`); scala-cli with Scala 3.9.0 compiles this file:

    ```scala
    sealed trait G[T]
    case class GI() extends G[Int]
    case class GS() extends G[String]
    def f[T](x: G[T], y: T): Any = x match
      case GI() | GS() => y
    ```

  - an implicit conversion found only for the qualifier's GADT approximation (`implicit conversion
    of a GADT-approximated qualifier`); scala-cli with Scala 3.9.0 compiles this file:

    ```scala
    import scala.language.implicitConversions
    sealed trait G[T]
    class R { def ext: Int = 1 }
    class K
    object K:
      given Conversion[K, R] = _ => R()
    case class GK() extends G[K]
    def f[T](x: G[T], y: T): Int = x match
      case GK() => y.ext
    ```

  - a block typed before its expected type is known, as an argument of an overloaded method, whose
    expression is a singleton that needs a GADT cast (`GADT cast of a singleton as a block's
    expression`); scala-cli with Scala 3.9.0 compiles this file:

    ```scala
    enum EQ[A, B]:
      case Refl[A]() extends EQ[A, A]

    object O:
      def h[T](t: T, u: Int): T = t
      def h[T](t: T, u: String): T = t

    def f[T](x: EQ[T, Int], a: Int): T = x match
      case EQ.Refl() => O.h[T]({ println("x"); a }, 1)
    ```

  - the shapes ADR 0119 names: a GADT parameter inside a union or intersection bound made equal to
    a type, and an extractor pattern outside a case that would need GADT mode.

  Typed differently from dotty (`just typer-compare`):
  - a block with a singleton expression as an `if` branch: one cast of the block where dotty has
    two, to the same type:

    ```scala
    enum EQ[A, B]:
      case Refl[A]() extends EQ[A, A]

    def f[T](x: EQ[T, Int], a: Int): T = x match
      case EQ.Refl() =>
        if a > 0 then { println("y"); a } else { println("z"); a }
    ```

  - `tests/pos/gadt-TypeSafeLambda`: `c.first` without dotty's cast (Upstream bugs to report, 22).
- **`typeChecks`.** Code the parser or typer reports an error for is `false`. A construct the typer
  refuses, with no error reported before it, refuses the program, where dotty may answer.
  `typeCheckErrors` gives `Nil` or the errors of inline expansions in the code, and is refused
  for an error of the parser or typer, as messages and positions differ from dotty's.
- **Macro annotations.** Refused: arguments other than literals or in several lists; on a
  definition the typer did not type (in a quote or library inline body), arguments at all; a class
  with an outer instance or erased parameters; a result that drops the annotated definition, returns
  a type, adds a top-level definition or one of another owner, or defines a symbol twice or defined
  elsewhere.
- **`@static`.** Refused: lazy values; members of an object whose companion is a value class or in
  another unit; methods and initial values that use the object's `this` or define local methods or
  classes; values whose companion class has lazy values and constructor parameters, parents but `Object`, or
  synthetic objects (Scala Native); a reference to a `@static` member of the library.
- **Erased definitions.** An erased argument or value must be pure by the inliner's
  `isElideableExpr` with `isPureExpr`'s type applications, or be an erased value of `scala.caps` or
  an inline value: some arguments dotty accepts are refused.
- **Tail recursion.** dotty's phases run `Erasure`, `TailRec`, `LambdaLift`; ours `lambda_lift`,
  `erasure`, `tailrec`. Before `lambda_lift`, `tailrec::name_vars` makes local methods loops and
  names the variables of members, which closures and local classes capture by value, so a closure
  made in an iteration sees that iteration's parameter (`tests/programs/tailcaptures`).
- **REPL commands.** `:jar`, `:dep`, `:doc` and `:settings` print `-- unsupported: <command>`, and
  `:help` lists only the commands that run. A `:load` of a path that exists but cannot be read
  prints `Couldn't read file "<path>": <error>`, and more than 16 nested `:load`s print
  `Couldn't load file "<path>": more than 16 nested :load commands`. `:imports` shows an identifier
  naming an inherited member of an imported object under its defining class, and a selector's bound
  as written.

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

Results:

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

Results:

| Outcome | Tests |
|---|---:|
| passed | 1502 |
| wrong output | 0 |
| crashed | 0 |
| timeout | 0 |
| lowering refused | 1 |
| typer refused | 3 |
| parser refused | 0 |
| macro refused | 0 |
| frontend crashed | 0 |
| fails in the VM | 32 |
| skipped | 173 |
| **total** | **1711** |

- **Lowering refused:** a boxing, unboxing or primitive conversion (`Parser`).
- **Typer refused:** an inline expansion of a type referring to a binding (`i13332shapeless`),
  `Printer.TypeReprStructure` of a type the frontend does not print (`i22649`), `typeChecks` of a
  non-constant string (`typeCheckErrors`).
- `Runtime.totalMemory`,
  `maxMemory` and `freeMemory`, which Scala Native's javalib lacks and the overlay adds (ADR
  0008), are typed through `libs/jdk-stubs/JdkMemberDeclarations.scala`.
- **`tests/run-macros`** (`just dotty-lower-report "" run-macros`): 311 of 321
  pass; the typer refuses two for classes Scala Native's javalib lacks, a platform limit:
  `java.lang.SuppressWarnings` (`annot-java-tree`) and `java.text.SimpleDateFormat`
  (`f-interpolator-tests`); 8 are skipped.
- **`-source 3.0` to `3.6` and `-migration` versions:** below 3.4 match types reduce as dotty's
  legacy matching does (a case with captures matched by a subtype comparison); below 3.6
  implicits rank by dotty's intermediate scheme; the typer refuses an implicit defined after the
  search below 3.6, a pair of implicits the two schemes rank differently in 3.6 and
  3.7-migration (dotty warns), and an inferred match type bound in 3.6-migration. Legacy
  constructs are errors, warnings or accepted by dotty's `MigrationVersion` table. `3.0-migration`
  (Scala 2 typing rules) and `-rewrite` are refused.
- **`-source future`:** typed as 3.9 with the parser's new-syntax rules; the typer refuses
  alphanumeric infix calls, types and extractor patterns not declared `infix`, and checks no
  other `future` rule (`future-migration` warns about them as dotty does).
- **Capture checking:** typed as dotty's typer types it (`ImpureFunctionN`, `@retains` trees);
  the typer's types drop `@retains`, `@retainsCap` and `@retainsByName` (the annotations are not
  refining and erase away; class files as dotty's). TASTy keeps a capture set of static
  definitions in the field of a class parameter, as dotty a type `retains[C]` or below source 3.9
  `new retains[C]()`, and refuses one of other references there and in pickled types. An inferred
  type tree keeps the capture sets of the type it is inferred from, emptied as dotty's
  `CleanupRetains` does (a type `T^` whole, so refused if `T` names other than static
  definitions), where that type is a function value's expected parameter type or the type of
  its right-hand side as the definitions it refers to tell it: a reference or call, with inferred
  type arguments that arguments of parameters of those types give, a conditional or match whose
  branches agree, a closure without an expected type. An overriding member keeps its overridden
  member's capture sets as they are (`explicifyTpt`). An inferred type in a definition that
  refers to a type with capture sets is refused if its own are not told so (unless a primitive
  type), as is a dependent function type whose refinement has capture sets, which dotty's
  function type parent approximates. The capture checker,
  which runs after the typer and leaves the NIR unchanged, is not run (#168). dotty pickles quotes before the capture
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
just jvm-lib-jar scalalib       # its class files and TASTy as a library jar; tmp/lib-jvm/scalalib/libs
```

`scivolo-jvm` writes class version 61 (Java 17) with stack map frames; its output is semantically
equivalent to scalac 3.9.0's, not byte-identical, and scala-cli compiles against the jars it writes.

Each test is compiled by `scivolo-jvm` (#142) and its `Test.main` called through reflection, as
dotty's test runner calls it, on `java -Xverify:all`; its output is compared with dotty's `.check`
file (run-macros: the one recorded in `tests/dotty-checks` where that exists), compiled from the
directory that holds `tests/`. Earlier compilation groups are compiled by scala-cli for the JVM;
for a macro call also for Scala Native, whose NIR the macro runs from in the VM (ADR 0112).
Skipped: tests for another platform or compiled partly by another compiler; Java sources are
compiled (#242). Failures are grouped by the refusal or the first exception line.

Results:

| Outcome | tests/run | tests/run-macros |
|---|---:|---:|
| passed | 1690 | 320 |
| wrong output | 0 | 0 |
| verify error | 0 | 0 |
| linkage error | 0 | 0 |
| crashed | 1 | 0 |
| timeout | 0 | 0 |
| frontend crashed | 1 | 0 |
| jvm refused | 0 | 0 |
| lowering refused | 0 | 0 |
| typer refused | 5 | 0 |
| skipped | 14 | 1 |
| **total** | **1711** | **321** |

- **Typer refused (run):** `i13332shapeless` and `typeCheckErrors` of the VM's report;
  `mapConserve` (a nullary overloaded method against an undetermined type);
  `varargs-extend-java` and `varargs-extend-java-2` (a concrete class with an unimplemented member).
- **Frontend crashed:** `named-tuples-strawman-2` (a stack overflow).
- **Tests with Java sources:** all pass.
- **Crash:** `lambda-serialization-gc` casts the application class loader to `URLClassLoader`,
  which fails for scalac's class files too, as the runner does not load tests through one.
- **Difference from dotty (open, not accepted):** a macro runs from its Scala Native build in the VM, so a
  macro implementation that reads the platform (`System.getProperty`, `scala.util.Properties`,
  runtime class names) sees Scala Native's; what it reflects on (`Symbol`s, types) is the JVM
  build's TASTy, as for dotty.

`just programs-jvm` compiles each of `tests/programs` with `scivolo-jvm`, runs it on
`java -Xverify:all` and compares its stdout and stderr, in one stream, with its `.check` (Scala
Native's output).
Results:

| Outcome | Programs |
|---|---:|
| passed | 301 |
| wrong output | 7 |
| linkage error | 1 |
| crashed | 3 |
| jvm refused | 17 |
| typer refused | 2 |
| **total** | **331** |

- **As scalac's class files on the JVM** (platform differences): wrong output `bottomarrays`,
  `universaljoins`, `exprforms` (array class names), `classtags` (`ClassTag` of a nested array),
  `staticinit` (object initialization order), `unitarrays` (`Array[Unit]` elements); crashes
  `closuretypes` (a package named `java.*`), `matchtypearrays` (`ClassCastException`), `resources`
  (no resource directory on the classpath); linkage error `protectedaccess` (`NoSuchMethodError`
  of `Copies.clone()`, an `invokeinterface` of `Object`'s protected `clone`).
- **Scala Native APIs:** refused unless Scala Native's jars are on the class path, where dotty
  does not find `scala.scalanative`; `overflow` is refused by the typer (a `scalanative.unsafe`
  macro call). `nativeannotations`, which only imports annotations no class file keeps, compiles
  and runs, while dotty refuses its imports.
- **Typer refused:** `hkerasure` (no implicit argument, as on Scala Native).

Scala's library (#222, ADR 0110): each of the 649 `.scala` files of `scala-library-3.9.0`'s
sources jar is compiled alone by `scivolo-jvm --scalalib` and its class files compared through
javap text, code and attributes apart, with dotty 3.9.0's from one run over the whole sources jar
with the library build's options without `-opt`. The published jar is no oracle: its build adds
the optimizer (1883 of its 3134 classes dotty also writes differ in code from dotty's) and takes
139 classes and 2.13's specialized ones from Scala 2.13. Results:

| Outcome | Files | Classes |
|---|---:|---:|
| same | 641 | 3254 |
| attributes differ | 5 | 5 |
| code differs | 2 | 11 |
| refused | 1 | |

- **Refused:** `scala/util/Properties.scala`, a nullary overloaded method against an undetermined
  type.
- **Code differs:** the pickled quotes of `scala.quoted.FromExpr` and `ToExpr` (strings of
  TASTy).
- **Attributes differ:** `TrieMap`, `HashSet`, `Vector`, `runtime.Tuples` and
  `ProcessBuilderImpl`.

`just jvm-lib-jar scalalib` packs those class files and TASTy into a library jar laid out as the
published one (ADR 0123): the classes scala3's build takes from Scala 2.13 and javac's come from
the published jar, a refused file's from dotty's run; `just dotty-jvm-report "" run
tmp/lib-jvm/scalalib/libs` types and runs tests/run against it, `just dotty-jvm-report "" run
tmp/lib-jvm/scalalib/libs/scala-library-3.9.0.jar` types them against the published jar and runs
them on ours. The jar is not ours alone: each report's first line counts its entries by origin.

### com-lihaoyi libraries from source

`just lib-source NAME` compiles each file of a library on its own and compares its NIR with
scala-cli's build of the whole module and, for the files that differ, with scala-cli's build of
that file alone. 1,777 of the 1,786 NIR files are the same as scala-cli's; 3 differ only in var
order; upickle's own 35 have no scala-cli oracle, as Scala 3.9 reports cyclic references in
`Api.scala`. Differences from both:

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

### Scala Native's library through the Rust frontend

`just lib-compile MODULE` lowers each file and compares its NIR with Scala Native's 3.9.0 build
(`just lib-oracle`); files that differ only in the order of var slots count as `var-order`.

| Module | Files lowered | NIR same | var-order | differ |
|---|---:|---:|---:|---:|
| javalib | 601 of 601 | 3,414 | 82 | |
| nativelib | 103 of 103 | 709 | 10 | |
| auxlib | 15 of 15 | 85 | | |
| clib | 21 of 21 | 115 | | |
| posixlib | 67 of 67 | 270 | | |
| windowslib | 34 | 130 | | |
| scalalib | 649 of 649 | 4,381 | 17 | 11 |
| test-interface-sbt-defs | 11 of 11 | 26 | | |
| test-interface-common | 13 of 13 | 109 | | |
| test-interface | 8 of 8 | 45 | | |
| junit-runtime | 51 of 51 | 149 | | |

clib, posixlib, scalalib, javalib and nativelib pass Scala Native's unit tests on our jars as on
the published ones (`just lib-jar-report`). `lib-compile` lowers javalib's `WindowsPath.scala`
and `WindowsPathParser.scala` as one unit, as Scala Native's build does: alone, the join of
`PathType`'s case objects read from TASTy also has their `Mirror.Singleton` parent and erases to
it, as in dotty's separate compilation. Likewise nativelib's `UnsafePackageCompat.scala`,
`unsafe/package.scala` and `unsafe/Ptr.scala`: alone, `unsafe/package$` lacks the call of the
trait's empty `$init$` (upstream bug 10), and the inline accessor `inline$rawptr$i1` sits at the
template's end, as dotty places the accessor of a member read from TASTy. Together, it sits at
`Ptr.rawptr`'s offset taken in the trait's source, inside a doc comment, as dotty places the
accessor of a member of another file of the run (`AccessProxies.accessorDef`). An offset past
the end of that source sits at its end, where dotty's column runs on along the last line
(case 1: a debug position, not representable in a unit's offsets). `lib-jar nativelib` lowers all
of nativelib's source directories as one unit, as Scala Native's build does.

`just lib-compile scalalib` lowers each file of Scala Native's scalalib alone against Scala's
library, its `LazyVals` replaced by Scala Native's (`scripts/scalalib_sources.py`). As in Scala
Native's build of the whole module, a selection of a synthetic member of a case class of Scala's
library (`(a, b).hashCode()`) selects the member it overrides (`Any.hashCode`): those members do
not exist yet while a class of the same run is typed. `ToExpr$ArrayOf*ToExpr$` (8),
`ToExpr$BigIntToExpr$`, `ToExpr$MapToExpr` and `FromExpr$MapFromExpr` differ in quote type
sharing (case 1, as in **Quote type sharing** above, types shared across the files of the
whole-module build); scala-cli's builds of `ToExpr.scala` and `FromExpr.scala` alone have their
quotes as ours, up to the source path. `lib-jar scalalib` lowers all files as one unit, as Scala
Native's build does: their quotes are as the oracle's. There, `ProcessImpl`'s `$N$` names of
pattern definitions (`CompoundProcess`'s `$1$`, `DummyProcess`'s `$3$` in dotty, `$3$` and `$2$`
in ours) follow the order in which the member classes of `ProcessImpl` are completed. Dotty
completes `CompoundProcess` first when a name lookup on `ProcessImpl`'s self type
(`ProcessImpl & Process.type`, `Process` of the same run) computes its members as seen from it
(`Typer.findRef`, `Types.findMember`, `ClassDenotation.addInherited`,
`SingleDenotation.computeAsSeenFrom` forcing the class's info); our lookup does not complete
member classes. `Process.scala`, `ProcessBuilderImpl.scala` and `ProcessImpl.scala` alone: dotty
`$3$`, ours `$2$` for `DummyProcess`; without `Process.scala` both `$2$`. Case 1: private names,
the same meaning. `DummyProcess`'s is a local of its constructor; `CompoundProcess`'s lazy value
is referenced only by `ProcessImpl$CompoundProcess` and its companion's `VarHandle` of the
same unit, and `ProcessImpl.scala` has no inline methods whose TASTy could refer to it.

## Native executables

`just aot-src-all`: 308 of 318 test programs build from source with `scivolo-aot`
and pass.

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

Results: of 104 files, 92 compile; all 867 tests in their 112 classes pass.

- **Not compiling with Scala 3 (12 files):** ambiguous givens (`BuildFromTest`,
  `DecoratorsTest`), missing `ClassTag`s, `==` between unrelated types, and overloads that
  Scala 3 resolves differently (`SortingTest`, `ArraySortingTest`, `RicherTest`).
- **Slowest:** `ListSetTest.hasTailRecursiveDelete` builds a 50,000-element `ListSet`, a
  quadratic 1.25 billion `==` on boxed `Int`s.
