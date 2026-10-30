//> using scala 3.9.0
import scala.language.implicitConversions

class Rich(x: Int) { def show: String = s"conv $x" }

class A(val x: Int)
object A {
  implicit def toRich(a: A): Rich = Rich(a.x)
  extension (a: A) def show: String = s"ext ${a.x}"
}

class BOps { extension (b: B) def show: String = s"ext ${b.x}" }
trait Low { given ops: BOps = BOps() }
class B(val x: Int)
object B extends Low {
  implicit def toRich(b: B): Rich = Rich(b.x)
}

class COps { extension (c: C) def show: String = s"ext ${c.x}" }
class C(val x: Int)
object C {
  implicit def toRich(c: C): Rich = Rich(c.x)
  given ops: COps = COps()
}

object extensionviews {
  def main(args: Array[String]): Unit = {
    println(A(1).show)
    println(B(2).show)
    println(C(3).show)
  }
}
