trait Shape
case class Circle(r: Int) extends Shape
case class Square(a: Int) extends Shape
trait T1
trait T2
trait A
trait B
class Sub extends Circle(0)

object UnionWidening {
  def c = args.isEmpty
  val args = List(1)
  val opt = if c then Some(1) else None
  val shape = if c then Circle(1) else Square(2)
  val mixed = if c then 1 else "a"
  val caught = try 1 catch { case _: Exception => "x" }
  val xs = List(1, "a")
  val ys = List(Some(1), None)
  def keep(x: Int | String) = x
  val kept = List(keep(2))
  def lit(x: 1 | 2) = x
  val f = (b: Boolean) => if b then Some(1) else None
  val g = (b: Boolean) => if b then List(1) else Vector(1)
  def nested[A <: Shape, B <: Shape](a: A, b: B) = if c then a else b
  def inter(x: T1 & T2 & A, y: T1 & T2 & B) = if c then x else y
  def inter2(x: T2 & T1 & A, y: T1 & T2 & B) = if c then x else y
  def classes(x: Sub & A, y: Circle & T1 & A) = if c then x else y
  def applied(x: List[Int] & A, y: List[String] & A) = if c then x else y
  def appliedAnd(x: List[Int] & Seq[Int], y: List[String]) = if c then x else y
  def m(x: Int) = x match {
    case 0 => Nil
    case _ => List(x)
  }
}
