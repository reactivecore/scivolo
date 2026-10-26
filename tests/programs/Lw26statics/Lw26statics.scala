import scala.annotation.static

class Foo:
  class Bar:
    def qwa = Bar.field
  object Bar:
    val field = 1

object Foo:
  @static def method = 1
  @static val field = 2
  @static var mutable = 3
  @static def accessor = field
  @static val greeting: String = "hi " + field

object Lw26statics:
  import Foo.*
  def main(args: Array[String]): Unit =
    println(method + field + mutable + accessor)
    mutable = 10
    println(Foo.mutable)
    println(greeting)
    println(new Foo().Bar.field)
