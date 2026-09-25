//> using scala 3.9.0

// Runs every JUnit test class the VM finds on its classpath. The VM provides the list of
// `…$scalanative$junit$bootstrapper` modules through two externs, so no reflection is needed.

import scala.scalanative.junit.Bootstrapper
import scala.scalanative.runtime.{Intrinsics, toRawPtr}
import scala.scalanative.unsafe.*
import scala.util.{Failure, Success, Try}

@extern object scalai:
  def scalai_bootstrapper_count(): CInt = extern
  def scalai_bootstrapper(i: CInt): CVoidPtr = extern
  def scalai_bootstrapper_name(i: CInt): CString = extern

def isAssumption(t: Throwable): Boolean =
  t.isInstanceOf[org.junit.AssumptionViolatedException] ||
    t.isInstanceOf[org.junit.internal.AssumptionViolatedException]

// Like `Try`, but also catches fatal throwables such as `LinkageError`, as JUnit does.
def capture[A](body: => A): Try[A] =
  try Success(body)
  catch { case t: Throwable => Failure(t) }

def attempt(body: => Any): Try[Unit] = capture(body).map(_ => ())

def runTest(b: Bootstrapper, name: String, expected: Class[?]): Try[Unit] =
  val wantException = expected != classOf[org.junit.Test.None]
  capture(b.newInstance()).flatMap { instance =>
    val result = attempt(b.before(instance)).flatMap { _ =>
      capture(b.invokeTest(instance, name)).flatMap(_.value.get.flatten)
    }
    val checked = (result, wantException) match
      case (Success(_), true)                            => Failure(new AssertionError(s"Expected exception: ${expected.getName}"))
      case (Failure(t), true) if expected.isInstance(t) => Success(())
      case (other, _)                                    => other
    checked.flatMap(_ => attempt(b.after(instance)))
  }

// Runs the test classes whose names contain one of the plain arguments (all if there are
// none), except those containing an argument given as `-part`.
@main def runner(filter: String*): Unit =
  val (excluded, included) = filter.partition(_.startsWith("-"))
  def selected(cls: String) =
    (included.isEmpty || included.exists(cls.contains)) && !excluded.exists(e => cls.contains(e.drop(1)))
  var (passed, failed, skipped) = (0, 0, 0)
  for i <- 0 until scalai.scalai_bootstrapper_count() do
    val cls = fromCString(scalai.scalai_bootstrapper_name(i))
    if selected(cls) then
      val b = Intrinsics.castRawPtrToObject(toRawPtr(scalai.scalai_bootstrapper(i))).asInstanceOf[Bootstrapper]
      if !b.testClassMetadata().ignored then
        attempt(b.beforeClass()) match
          case Failure(t) =>
            failed += 1
            println(s"FAIL $cls beforeClass: $t")
          case Success(_) =>
            for t <- b.tests() do
              if t.ignored then skipped += 1
              else
                runTest(b, t.name, t.annotation.expected) match
                  case Success(_) =>
                    passed += 1
                    println(s"PASS $cls.${t.name}")
                  case Failure(e) if isAssumption(e) =>
                    skipped += 1
                    println(s"SKIP $cls.${t.name}")
                  case Failure(e) =>
                    failed += 1
                    println(s"FAIL $cls.${t.name}: $e")
            attempt(b.afterClass())
  println(s"passed $passed, failed $failed, skipped $skipped")
