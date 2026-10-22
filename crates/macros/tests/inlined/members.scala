//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Members.*

sealed trait Shape
case class Circle(r: Double) extends Shape
case class Rect[T](w: T, h: T)(val label: String) extends Shape
object Empty extends Shape

trait Named:
  def name: String = "n"

class Service(val id: Int) extends Named:
  def run(times: Int)(using ctx: String): Unit = ()
  def poly[A, B](a: A, b: B): A = a
  def plain: Int = 1

object Service

object Members:
  val a = structure[Shape]
  val b = structure[Circle]
  val c = structure[Rect[Int]]
  val d = structure[Service]
  val e = methods[Service]
  val f = inherited[Service]("name")
  val g = inherited[Service]("toString")
  val h = required
  val i = structure[Service.type]
