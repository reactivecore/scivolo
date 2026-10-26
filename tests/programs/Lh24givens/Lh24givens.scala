import scala.compiletime.summonInline

trait Show[T]:
  def show(t: T): String

class Tagged[T](tag: String) extends Show[T]:
  def show(t: T): String = s"$tag $t"

implicit val intShow: Show[Int] = Tagged("int")

inline def shown[T](t: T): String =
  implicit val local: Show[String] = Tagged("local")
  summonInline[Show[T]].show(t)

inline def shadowed: String =
  implicit val inner: Show[Int] = Tagged("inner")
  summonInline[Show[Int]].show(0)

@main def Lh24givens(): Unit =
  println(shown(1))
  println(shown("a"))
  println(shadowed)
