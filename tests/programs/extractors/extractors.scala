//> using scala 3.9.0
case class P(x: Int, name: String)
sealed trait Shape
case class Circle(r: Double) extends Shape
case class Rect(w: Double, h: Double) extends Shape
case object Empty extends Shape

object Even:
  def unapply(n: Int): Boolean = n % 2 == 0
object Half:
  def unapply(n: Int): Option[Int] = if n % 2 == 0 then Some(n / 2) else None
object Split:
  def unapply(s: String): Option[(String, String)] =
    val i = s.indexOf(':')
    if i < 0 then None else Some((s.substring(0, i), s.substring(i + 1)))

object E:
  def show(p: P): String = p match
    case P(0, n) => "zero " + n
    case P(x, "a") => "a " + x
    case P(x, n) => n + x
  def area(s: Shape): Double = s match
    case Circle(r) => 3.0 * r * r
    case Rect(w, h) => w * h
    case Empty => 0.0
  def opt(o: Option[Int]): Int = o match
    case Some(n) if n > 0 => n
    case Some(_) => -1
    case None => 0
  def nested(x: Any): String = x match
    case P(n, s) => "p" + n + s
    case Some(P(n, _)) => "sp" + n
    case _ => "?"
  def pairs(t: (Int, String)): String = t match
    case (0, s) => "zero " + s
    case (n, s) => s + n
  def custom(n: Int): String = n match
    case Even() => "even"
    case _ => "odd"
  def half(n: Int): Int = n match
    case Half(h) => h
    case _ => -1
  def split(s: String): String = s match
    case Split(k, v) => k + "=" + v
    case _ => "none"
  def mixed(c: Boolean): Any = if c then 1 else "s"
  def bound(x: Any): String = x match
    case p @ P(1, _) => "one " + p
    case _ => "other"

@main def extractors(): Unit =
  println(E.show(P(0, "z")))
  println(E.show(P(2, "a")))
  println(E.show(P(3, "q")))
  println(E.area(Rect(2.0, 3.0)))
  println(E.area(Empty))
  println(E.opt(Some(5)))
  println(E.opt(None))
  println(E.nested(Some(P(4, "x"))))
  println(E.pairs((0, "a")))
  println(E.pairs((7, "b")))
  println(E.custom(4))
  println(E.half(10))
  println(E.half(3))
  println(E.split("k:v"))
  println(E.split("kv"))
  println(E.bound(P(1, "o")))
  println(E.mixed(false))
