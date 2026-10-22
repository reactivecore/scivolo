//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Symbols.*

sealed trait Shape
case class Circle(r: Double) extends Shape
abstract class Base
enum Color:
  case Red

object Symbols:
  val a = flagsOf[Shape]
  val b = flagsOf[Circle]
  val c = flagsOf[Base]
  val d = flagsOf[Color]
  val e = flagsOf[Option[Int]]
  val f = flagsOf[Some[Int]]
  val g = flagsOf[None.type]
  val h = flagsOf[Int]
  val i = flagsOf[Symbols.type]
  lazy val j = owners
  def k(n: Int) = owners
  var l = owners
  class Inner:
    val m = owners
    println(owners)
  given n: String = owners
