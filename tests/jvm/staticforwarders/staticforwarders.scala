import scala.annotation.{static, targetName}

class Foo
object Foo:
  @static def foo: String = "foo"
  @targetName("foo") def fooBincompat: String = foo + "!"

object Staticforwarders:
  def main(args: Array[String]): Unit =
    println(Foo.foo)
    println(Foo.fooBincompat)
