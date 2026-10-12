trait Shape
case class Circle(r: Int) extends Shape
case class Square(a: Int) extends Shape

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
  def m(x: Int) = x match {
    case 0 => Nil
    case _ => List(x)
  }
}
