class Base[T](val v: T)

def show(x: Int) = "int"
def show(x: Long) = "long"

object Outer { type U = Long }
import Outer.*

object M {
  import B.C.*
  class R extends Base[U](1)
  def mk = new R
}

object B {
  val r: M.R = null
  def w = r.v
  object C { type U = Int }
  export M.mk
}

object Test {
  def main(args: Array[String]): Unit = println(show(new M.R().v))
}
