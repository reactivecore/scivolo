import scala.language.experimental.modularity
import scala.language.future

abstract class Foo(tracked val d: String):
  def show: String = d

class Bar extends Foo("bar")
class Qux extends Foo(List("q").head)

enum Kind(tracked val label: String):
  case A() extends Kind("a")

object ParentRefinements:
  def main(args: Array[String]): Unit =
    val b = new Bar
    val s: "bar" = b.d
    println(s)
    println(b.show)
    println((b: Foo).d)
    println(new Qux().d)
    println((Kind.A(): Kind).label)
    println(classOf[Bar].getDeclaredMethods.length)
