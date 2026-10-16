//> using scala 3.9.0
import scala.language.dynamics

class Record(self: String) extends Dynamic:
  def selectDynamic(name: String): Record = Record(s"$self.$name")
  def applyDynamic[T](name: String)(args: Any*): Record = Record(s"$self.$name${args.mkString("(", ", ", ")")}")
  def applyDynamicNamed(name: String)(args: (String, Any)*): Record =
    Record(s"$self.$name${args.mkString("(", ", ", ")")}")
  def updateDynamic(name: String)(value: Any): Record = Record(s"$self.$name = $value")
  def update(key: Int, value: Int): Record = Record(s"$self($key) = $value")
  override def toString = self

@main def dynamic(): Unit =
  val r = Record("r")
  println(r.a.b)
  println(r.f(1, "x"))
  println(r.g[Int]())
  println(r.h(x = 1, 2))
  println(r(3))
  println(r.v = 4)
  println(r.w(5) = 6)
