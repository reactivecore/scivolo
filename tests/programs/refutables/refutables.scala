//> using scala 3.9.0
class Box[T](val v: T)
object Box:
  def unapply[T](x: Any): Option[Box[T]] =
    if x.isInstanceOf[Box[?]] then Some(x.asInstanceOf[Box[T]]) else None

object Defs:
  val x0 = 0
  def run(x: Object): Int =
    val Array(who, what*) = "a b c".split(" ")
    val List(_*) = List(1, 2)
    val Seq(1, 2) = LazyList(1, 2)
    val s @ (_s: String) = x
    val (c, List(`x0`, _, a), b) = (5, List(0, 1, 2), 3)
    val Some(y) = Option(3)
    val n :: rest = List(4, 5)
    val Box(box) = Box("in")
    println(who + what.mkString("[", ",", "]") + s + _s)
    c + a + b + y + n + rest.size + box.v.toString.length
  def failing(): String =
    try
      val Some(z) = Option.empty[Int]
      "matched " + z
    catch case e: MatchError => "no match"

object Binds:
  def num(a: Int | Double): Int = a match
    case _: (Float | Boolean) => 1
    case _ => 0
  def gen[T](x: T): Int = x match
    case i: Int => i + 1
    case s: String => s.length
    case _ => -1
  def single(h: Any): Any = h match
    case Nil => "nil"
    case List(from) => from
    case _ => "other"
  def anyRef(x: AnyRef): Int = x match
    case List(_) => 1
    case _ => 0

@main def refutables(): Unit =
  println(Defs.run("str"))
  println(Defs.failing())
  println(Binds.num(4))
  println(Binds.num(4.5))
  println(Binds.gen(41))
  println(Binds.gen("four"))
  println(Binds.gen(4.0))
  println(Binds.single(Nil))
  println(Binds.single(List("one")))
  println(Binds.single(List(1, 2)))
  println(Binds.anyRef(List(1)))
  println(Binds.anyRef(Nil))
