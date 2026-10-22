import scala.reflect.Selectable.reflectiveSelectable

trait Box { type T; def get: T }
class IntBox extends Box { type T = Int; def get = 1 }

object Main {
  type IntOf = Box { type T = Int }
  type Small = Box { type T <: AnyVal }
  type Sized = { val size: Int; def length(): Int; def at(i: Int, j: Int): Char }
  type Pick = [A] => (xs: List[A]) => Option[A]
  val pick: Pick = [A] => (xs: List[A]) => xs.headOption
  def first(b: IntOf): Int = b.get
  def main(args: Array[String]): Unit = {
    val b: IntOf = new IntBox
    println(first(b))
    println(pick(List(1, 2)))
  }
}
