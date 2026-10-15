trait It { type T; def next: T; def all: List[T] }
trait Mid extends It
class SI extends Mid {
  type T = Char
  def next = { val x = 'c'; x }
  def all = List(next)
}
object Test {
  def main(args: Array[String]): Unit = println(new SI().next)
}
