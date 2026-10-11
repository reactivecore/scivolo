import scala.annotation.{elidable, static, targetName, unroll}
import scala.annotation.elidable.FINEST

class Box[@specialized(Int) A](@transient val a: A)

object Box:
  @static def unit: Int = 0

class C:
  def f(x: Any): Int = 1
  @targetName("g") def f(x: String): Int = 2
  @deprecated("use f", since = "1.0") def h: Int = f("a")
  @transient lazy val l: Int = h
  @volatile var v: Int = 0
  @elidable(FINEST) def e(): Unit = ()
  final def u(s: String, @unroll n: Int = 1): String = s + n
