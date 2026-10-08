//> using scala 3.9.0
object G:
  def a: Int = Seq().length
  def b: Seq[Any] = Option(1).map(_ => List()).getOrElse(Nil)
  def c: Iterable[Int] = Seq()
  def d: Int = (if a > 0 then List() else Seq(1)).length
  def e: Seq[Seq[Int]] = Seq(Seq(), List())
  def f(xs: Int*): Int = xs.length
  def g: Int = f(List()*)
  def h: Any = try List() finally ()
  def i: Seq[Int] = { val _ = 1; Seq() }: Seq[Int]
  def v: Int = Vector().length

object E:
  val e: List[Int] = List()
  val s = Seq()
  def k: List[Int] =
    val y = 1
    List()
  val a = Array(1, 2)
  val b = Array("x", "y")
  def m(xs: List[Int]*): Int = xs.length
  def n: Int = m(List(), List(1), Nil)

@main def emptyliterals(): Unit =
  println(G.a + G.b.length + G.c.size + G.d + G.e.length + G.g + G.i.length + G.v)
  println(G.h)
  println(E.e.length + E.s.length + E.k.length + E.a.length + E.b.length + E.n)
