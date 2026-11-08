# 46 — JVM bytecode backend (#142)

`D/` = `ref/scala3/compiler/src/dotty/tools/dotc/`, `J/` = `ref/scala3/compiler/src/dotty/tools/backend/jvm/`,
`N/` = `ref/scala-native/nscplugin/src/main/scala-3/scala/scalanative/nscplugin/`, `L/` = `crates/lower/src/`.
Context: issue #142 and its first assessment (2026-10-27); ADR 0010 (faithful post-typer phases),
ADR 0020 (inlining in the typer), ADR 0067 (TASTy writer), ADR 0072/0082 (JDK declarations);
`ROADMAP.md` Phase 5 lists JVM bytecode as optional, so the first MR needs an ADR (0101).

## 1. Target

1. `scivolo-jvm FILE… -d DIR` writes class files (and a jar) that are semantically equivalent to
   scalac 3.9.0's, not byte-identical: same class names, descriptors, access flags, forwarders,
   bridges, generic `Signature`s, `InnerClasses`, runtime-visible annotations, readable TASTy.
   May differ: constant pool order, member and instruction order, `StackMapTable` frames, private
   lambda method names, line number tables where our spans differ.
2. Class file version 61 (Java 17): dotty's default is the least supported target
   (`J/BCodeHelpers.scala:42-56`, `D/config/ScalaSettingsProperties.scala:8`); string concatenation is
   `invokedynamic makeConcatWithConstants` (`J/BCodeIdiomatic.scala:166-178`), lambdas
   `invokedynamic LambdaMetafactory` (`J/BCodeBodyBuilder.scala:1769`).
3. Runs on the JVM against `ref/libs/scala-library-3.9.0.jar` (3,662 class files, the JVM
   standard library we already type against) — no JDK licence question: we read JDK class files,
   we ship none.
4. Targets in order: dotty's `tests/run` on a JVM; the standard library; the compiler (needs a Java
   parser, out of scope here).

## 2. Reusable as is

1. Parser, desugaring, typer, inlining, macros: all of it.
2. The lowering's phases `L/phases/pipeline.rs:8-122`: every dotty phase up to `SelectStatic`,
   which is exactly what `GenBCode` consumes (`D/Compiler.scala:62-152`; nscplugin's `GenNIR` runs
   after `MoveStatics`, before `GenBCode`, `N/GenNIR.scala:15-16`), bar the per-backend phases of §3.
3. The TASTy pickler `crates/typer/src/pickle/` (quotes today) for the `.tasty` files and the
   `TASTY` class attribute; the typer's trees before inlining are what it needs (ADR 0020).
4. `zip` is already a workspace dependency (jars, jmods).
5. The dotty test driver `scripts/dotty-tests.py` and the per-test `.check` files
   (`tests/programs/*/`, `tests/aot/*/`).

## 3. What differs per backend

Phases and generator parts whose JVM shape is not the Scala Native one. `S` = in the shared
phases behind a `Backend` switch, `G` = in the JVM generator only.

| | dotty | Scala Native / ours | JVM shape |
|---|---|---|---|
| S | `LazyVals` `D/transform/LazyVals.scala` (706) | `N/AdaptLazyVals.scala`, `L/phases/lazy_vals/` (`objCAS` on the field) | `OFFSET$_m_N` static fields set by `LazyVals.getOffsetStatic`, `LazyVals.objCAS(this, offset, …)` |
| S | `MoveStatics` `D/transform/MoveStatics.scala` | `L/phases/static_members.rs`, `static_values.rs` (ADR 0044/0047: fields of the object) | static fields and methods of the companion class, `<clinit>` |
| S | `CompleteJavaEnums` `D/transform/CompleteJavaEnums.scala` | `L/phases/java_enums.rs` "without the static forwarders" | `values`/`valueOf` forwarders, `$values` |
| S | — | `L/phases/native_interop.rs` (`N/PostInlineNativeInterop.scala`) | skipped |
| G | `Flatten` `D/transform/Flatten.scala` (68) + `InnerClasses`/`EnclosingMethod` (`J/BackendUtils.scala`, `J/BCodeHelpers.scala`) | flat NIR names | top-level class files `Outer$Inner.class` with the `InnerClasses` table |
| G | closures: `J/BCodeBodyBuilder.scala:1769` `genInvokeDynamicLambda`, `J/BackendUtils.scala:46` `$deserializeLambda$` | `N/NirGenExpr.scala:240` `genClosure` (anonymous classes), `L/codegen/closure.rs` | `invokedynamic` with `LambdaMetafactory.altMetafactory`, serializable flag and bridges |
| G | mirror classes and static forwarders `J/BCodeHelpers.scala:457,560` | NIR forwarders (`L/codegen/statics.rs`) | `Hello.class` with `public static` forwarders of `Hello$` |
| G | `genSynchronized`, `genLoadTry` `J/BCodeSyncAndTry.scala` (475) | NIR `try`, `L/codegen/tries.rs`, `synchronized.rs` | exception tables, `monitorenter/exit` with a handler |
| G | string concat `J/BCodeBodyBuilder.scala:1342` | `L/codegen/concat.rs` | `invokedynamic makeConcatWithConstants` (≤ 200 slots) |
| G | `GenericSignatures.scala` (595), `emitAnnotations`, `Exceptions`, `Deprecated`, `SourceFile`, `TASTY`, `Scala` attributes (`J/BCodeHelpers.scala`) | dropped | emitted; signatures from the types before erasure (ADR 0010's per-phase infos) |
| G | `BTypes.scala` (889), `WellKnownBTypes.scala` | NIR types | descriptors, `jvmWiseLUB` for frames |

Unchanged for both: `SelectStatic`, `ElimStaticThis`, `Constructors`, `Mixin`, `Memoize`,
`LambdaLift`, `Erasure` incl. `closures.rs`, `TypeTestsCasts`, `InterceptedMethods`, `@main`
proxies, `SyntheticMembers` (`writeReplace`, `L/phases/synthetic.rs:60`).

## 4. Code generator

1. **Input**: the lowered trees (`Lower.tpd`, symbols in `Lower.u`) after `phases::run`, not NIR:
   NIR has no generic signatures, no `invokespecial`/`invokedynamic` distinction, no inner-class
   table, and boxes closures into classes. Port `J/BCodeSkelBuilder.scala` (950: class skeleton,
   fields, `<clinit>`, locals, labels), `J/BCodeBodyBuilder.scala` (1,879: expressions, calls,
   `genLoadTo`), `J/BCodeIdiomatic.scala` (621: instruction helpers), `J/BCodeSyncAndTry.scala`,
   `J/BCodeHelpers.scala` (693: forwarders, mirrors, annotations), `J/GenericSignatures.scala`,
   `J/BTypes.scala`; ≈ 6,000 Scala lines, no optimizer (`J/BackendUtils.scala` only in part).
2. **Crates** (backend-invariant code out of NIR crates, as the issue's note asks):
   - `crates/classfile` (lib `classfile`): the class file model and writer, later the reader
     (§5); depends on nothing of ours.
   - `crates/jvm` (lib `jvmgen`, bin `scivolo-jvm`): the generator and the jar writer; depends on
     `lower`, `typer`, `tasty`, `classfile`, `zip`.
   - `crates/lower`: `phases/` gets a `Backend { Native, Jvm }` in `Lower`; the entry
     `lower_trees` runs the phases without `codegen::unit`. Later `codegen/` (NIR generation,
     ≈ 50 files) moves to `crates/nirgen`; a mechanical MR on its own, after the JVM generator exists.
3. **Class file writer: own vs crate.** `noak` (reader/writer, visitor API) and `ristretto_classfile`
   (full model of every attribute, reader/writer) write bytes, but neither computes stack map
   frames, max stack or labels/jump offsets, which is what the generator needs (ASM's `visitLabel`/
   `visitJumpInsn` level). An own writer is ≈ 800 lines: constant pool (dedup by `FxHashMap`),
   code buffer with labels and 16-bit jump fix-ups (`goto_w` fallback later), attributes, `max_stack`
   by tracking the pushes and pops per opcode. Recommended: own, under `crates/classfile`.
4. **Stack map frames** (`StackMapTable`, mandatory from version 50): dotty lets ASM compute them
   (`COMPUTE_MAXS | COMPUTE_FRAMES`, `J/PostProcessor.scala:142`) and only supplies the class LUB
   (`getCommonSuperClass` → `jvmWiseLUB`, `J/PostProcessor.scala:159-174`, `J/BTypes.scala`).
   Options:
   a. Port ASM's dataflow (`Frame.java`, `MethodWriter.computeAllFrames`, BSD-3, ≈ 1,100 lines):
      basic blocks, abstract interpretation per opcode over verifier types (`Top`, `Integer`,
      `Float`, `Long`, `Double`, `Null`, `UninitializedThis`, `Uninitialized(offset)`,
      `Object(class)`), merge with LUB from the class hierarchy (our `Universe` for Scala and
      javalib classes; §5 for the rest). Frames may differ from ASM's where the LUB differs; the
      verifier accepts both. Recommended; ASM goes into `acknowledges.md`.
   b. Frames from the generator (javac's way: track locals and stack at each label): needs definite
      assignment at merges, which is the same dataflow again, in a less separable place.
   c. Class version 49 needs no frames but forbids `invokedynamic`: not an option.
   Frames are needed only in methods with branches or handlers, so the first slices
   (straight-line code) run without them.
5. **Jar**: `zip` with `META-INF/MANIFEST.MF`, class files, `.tasty` files; scala-cli's
   `--power package --library` is the oracle for the layout.

## 5. JDK symbols

1. Today JDK classes are javalib's TASTy with the JDK's declarations over them (ADR 0072/0082).
   For the JVM the descriptors must be the JDK's: the declaring class and the exact overload.
   The declarations fix the members scripts use; a javalib-only member (ADR 0072: hidden only
   when a declaration shares its name) becomes a `NoSuchMethodError` on the JVM, which the
   oracle of §6 catches. Enough for the first steps.
2. Then a port of `D/core/classfile/ClassfileParser.scala` (1,410; attributes `Signature`,
   `InnerClasses`, `Exceptions`, `RuntimeVisible(Parameter)Annotations`, `AnnotationDefault`,
   `ConstantValue`, `Deprecated`, `TASTY`/`Scala` for Scala-compiled classes) into
   `crates/classfile` (reader) and `crates/tasty/src/symbols` (entering), used first for the
   frame LUB (class hierarchy) and for JDK classes javalib lacks, later for `--dep` JVM jars.
3. Where the class files are: `jmods/java.base.jmod` etc. (zip with a 4-byte header, classes
   under `classes/`; present in the homebrew JDK 21 here), `lib/ct.sym` (zip of stripped
   `.sig` class files per release, what dotty's `-release` reads), or `lib/modules` (jimage, own
   format, resources compressed only by `jlink --compress`). ct.sym lacks the JDK's own
   release, Debian's headless JDK the jmods: `lib/modules` first, ct.sym as the fallback
   (ADR 0106).

## 6. Testing oracle

1. `just jvm-fixture FILE`: `scala-cli --power compile -S 3.9.0 --server=false -d tmp/jvm/<name>/ref FILE`
   (scala-cli and JDK 21 are on the machine and in CI: `just sn-tests`, `just pickle-fixture`).
2. `just jvm-check FILE`: `scivolo-jvm` into `tmp/jvm/<name>/out`, then for each class file
   `javap -p -s -c` of both, normalized (`#N` constant-pool indexes dropped, members sorted), diffed;
   `javap -v` filtered to `Signature`, `InnerClasses`, `RuntimeVisibleAnnotations`, `Exceptions`
   and `TASTY` lines for the attributes. `StackMapTable` and `LineNumberTable` are not compared.
3. `just jvm-run FILE`: `java -cp tmp/jvm/<name>/out:ref/libs/scala-library-3.9.0.jar <main>`
   against the `.check` file, as `just lower-run` does in the VM. Also `java -Xverify:all`.
4. Later: `scripts/dotty-tests.py` gets a `jvm` mode (dotty's `tests/run` through `scivolo-jvm`,
   run on `java`, report under `tmp/dotty`); separate compilation in both directions (ours
   against scalac's class files via §5.2, scalac against ours via `-cp tmp/jvm/.../out`);
   `tasty-inspector`/`scala-cli` reading our `.tasty`.

## 7. Staged plan

Each step is one MR of ≤ ~500 LOC with its gates; acceptance named per step.

1. **Class file writer and `Hello`** (ADR 0101: the JVM backend's crates and the `Backend`
   switch; roadmap row). `crates/classfile/src/{lib.rs, pool.rs, code.rs, writer.rs}`: constant
   pool (`Utf8`, `Class`, `String`, `Integer`, `NameAndType`, `Fieldref`, `Methodref`,
   `InterfaceMethodref`), class with flags, super, interfaces, fields, methods, `Code`
   (`max_stack`/`max_locals` tracked, no frames), `SourceFile`; opcodes of straight-line code
   (`aload`/`astore`, `ldc`, `getstatic`/`putstatic`, `new`, `dup`, `invoke{special,static,
   virtual,interface}`, `return`/`areturn`, `checkcast`). `crates/jvm/src/{main.rs, gen/mod.rs,
   gen/class.rs, gen/method.rs, gen/mirror.rs}`: CLI as `scivolo-lower`'s (`FILE… -d DIR`,
   `--cp`), generator for a module class with fields, `<clinit>`, constructor, methods whose
   bodies are calls, literals and module loads, plus the mirror class with static forwarders
   (`J/BCodeHelpers.scala:457,560`). `crates/lower`: `pub fn lower_trees` (phases without
   `codegen::unit`) and `pub use` of what the generator reads from `Lower`, ≤ 40 lines.
   Justfile: `jvm-fixture`, `jvm-check`, `jvm-run`. Test: `tests/jvm/hello/hello.scala`
   (`object Hello { def main(args: Array[String]): Unit = println("Hello") }`, `hello.check`), a
   crate test runs it when `java` is on the PATH. Acceptance: `just jvm-run tests/jvm/hello/hello.scala`
   prints `Hello`; `just jvm-check` shows no difference for `Hello` and `Hello$`.
2. **Expressions and locals**: `BCodeBodyBuilder` for literals of every primitive, locals,
   arithmetic and comparisons, boxing (`BoxesRunTime`), `if`/`while`/labelled blocks, arrays,
   `instanceof`/`checkcast`, with `max_stack` and labels, without frames in version 49 for this
   step only (`-Xverify` passes with the inference verifier). Acceptance: `tests/jvm/arith`,
   `tests/jvm/loops` run and `jvm-check` without differences in `javap -c`.
3. **Stack map frames**: dataflow of §4.4a over our code buffer, LUB from `Universe`; class
   version back to 61. Acceptance: steps 1–2 verify with `java -Xverify:all`; the frames of
   `tests/jvm/*` match ASM's on the fixtures (`javap -v` diff, informative).
4. **Classes**: plain classes, traits (interfaces with default methods, `$init$`), `super`
   calls, constructors with outer fields, `InnerClasses`/`EnclosingMethod`, case classes.
   Acceptance: `tests/programs/{abstractvals,arrows,…}` chosen for no lambdas; `jvm-check`.
5. **Lambdas**: `invokedynamic` with `LambdaMetafactory`, `$deserializeLambda$`, `BootstrapMethods`,
   string concat via `makeConcatWithConstants`. Acceptance: `tests/programs` with closures and
   interpolators.
6. **Try, synchronized, pattern matching**: exception tables, `genLoadTry`, `monitorenter`;
   `lookupswitch`/`tableswitch`. Acceptance: tests with `try`/`match`.
7. **Backend switch in the lowering**: `Backend::Jvm` for `LazyVals`, `MoveStatics` and
   `CompleteJavaEnums` (§3, S rows), `native_interop` skipped. Acceptance: lazy vals, `@static`,
   Java enums run; NIR oracle unchanged (`just nir-cmp`).
8. **Attributes**: `GenericSignatures`, runtime annotations, `Exceptions`, `Deprecated`,
   `MethodParameters` where dotty emits them. Acceptance: `javap -v` attribute diff empty.
9. **TASTy and jar**: `.tasty` per top-level class from the typer's pre-inlining trees, the
   `TASTY` attribute, jar with manifest; scala-cli compiles a user of our jar. Acceptance:
   `scala-cli compile` against our jar; `tasty-inspector` reads it.
10. **Class file reader** (§5.2) for the frame LUB and JDK classes javalib lacks; jmods and
    ct.sym. Acceptance: a test using `java.util.stream` compiles and runs.
11. **dotty's `tests/run` on the JVM**: `scripts/dotty-tests.py jvm`; report in `docs/conformance.md`.
    Then the stdlib (research 36's pipeline against `scala-library-3.9.0.jar`) as its own issue.
12. **`codegen/` out of `crates/lower`** into `crates/nirgen`: mechanical, once steps 1–7 show the
    seam; acceptance `just gates`, `just nir-cmp` on `tests/programs`.

## 8. Out of scope

1. The compiler as a target (Java parser, `-Yexplicit-nulls` on the JVM stdlib).
2. An optimizer (`-opt`), the inliner and closure elimination.
3. Scala 2 pickles (`ScalaSig`) in the reader; Scala 2 jars on the classpath.
