class Inv[T](t: T)
object M {
  type L = List[Int]
  val xv: Predef.String = "a"
  def a = new Inv(xv)
  def y: L = Nil
  def b = new Inv(y)
  val xl: L = Nil
  val c = Set(xl)
  def id[T](t: T): T = t
  def d = id(xl)
  def e: Any = new Inv(xl)
  def f(): Unit = { val w = new Inv(xl); () }
}
