import scala.language.experimental.modularity
import scala.language.future

abstract class Foo(tracked val d: String)
class Bar extends Foo("bar")
class Qux extends Foo(List("q").head)

enum Kind(tracked val label: String):
  case A() extends Kind("a")

val s: "bar" = Bar().d
val q: String = Qux().d
