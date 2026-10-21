import scala.annotation.StaticAnnotation

class num(n: Int) extends StaticAnnotation
class fun(f: Int => Int) extends StaticAnnotation
class of[T](x: T) extends StaticAnnotation

object A:
  val name = 1
  def f(@num(name) name: String): String = name
  @fun(x => x.+(1)) def g: Int = 1
  def h[T](@of[T](null.asInstanceOf[T]) t: T): T = t
  @fun(new Function1[Int, Int] { def apply(x: Int) = x }) val k = 2

@fun((y: Int) => y.*(2)) class B(@num(A.name) val v: Int)
