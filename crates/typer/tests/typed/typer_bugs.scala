import scala.reflect.ClassTag

class Foo(val n: Int)

object Test {
  def sibling = { { val it = 1; println(it) }; { val it = 2; println(it) } }

  def inner(using f: Foo): Int = f.n
  def outer(using Foo): Int = inner + 1
  def implicitParam(implicit f: Foo): Int = inner

  def less[T: Ordering](x: T, y: T): Boolean = summon[Ordering[T]].lt(x, y)
  class Box[T: ClassTag](val x: T) { def tag = summon[ClassTag[T]] }
}
