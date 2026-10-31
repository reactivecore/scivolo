//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Terms.*
import scala.quoted.runtime.Patterns.*

object ExprMatch:
  val a = exprMatch(List(1, 2).foreach(x => println(x)), { @patternType type T; patternHole[List[Int]].foreach[T](patternHole[Int => T]) })
  val b = exprMatch(List(1, 2).foreach(x => println(x)), { @patternType type T = Unit; patternHole[List[Int]].foreach[T](patternHole[Int => T]) })
  val c = exprMatch(List(1, 2).foreach(x => println(x)), { @patternType type T <: String; patternHole[List[Int]].foreach[T](patternHole[Int => T]) })
  val d = exprMatch({ val a: Int = 4; val b: String = "x" }, { @patternType type T; { val a: T = patternHole[T]; val b: T = patternHole[T] } })
  val e = exprMatch({ val a: Int = 4; val b: String = "x" }, { @patternType type T <: Int; { val a: T = patternHole[T]; val b: T = patternHole[T] } })
  val f = exprMatch((x: Int) => x.toString, { @patternType type T; patternHole[T => T] })
  val g = exprMatch(List(1).map(x => x.toDouble).map(y => y.toString), { @patternType type T; @patternType type U; @patternType type V; patternHole[List[T]].map(patternHole[T => U]).map(patternHole[U => V]) })
