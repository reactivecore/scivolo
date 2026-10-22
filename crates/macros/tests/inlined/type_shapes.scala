//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.TypeShapes.*

trait Animal
trait Pet extends Animal
class Dog extends Pet
case class Box[T](v: T)
object Single
type Alias = Box[Int]

object TypeShapes:
  val a = describe[Dog]
  val b = describe[Box[String]]
  val c = describe[Alias]
  val d = describe[Single.type]
  val e = describe[Int]
  val f = facts
