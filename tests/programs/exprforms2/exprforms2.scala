//> using scala 3.9.0
object Seqs:
  sealed class A
  case object B extends A
  case class C(x: Int) extends A
  def single[T](x: T): List[T] = List(x)
  def f[T](x: Option[T]) = x.fold(List.empty[T])(List(_))
  def g(x: Option[A]) = x.fold(-1) {
    case B    => 0
    case C(x) => x
    case _    => -2
  }
  def run(): Unit =
    println(single("s"))
    println(f(Some(3)))
    println(g(Some(C(4))))
    println(g(None))

object Branches:
  def nulls(b: Boolean): String =
    val x = if b then null else "good"
    if x == null then "null" else x + "!"
  def run(): Unit =
    println(nulls(true))
    println(nulls(false))

object Interpolations:
  def made(): StringContext = { println("made"); StringContext("<", ">") }
  def raw(sc: StringContext, x: Int): String = sc.raw(x)
  def run(): Unit =
    val a = "a"
    println(StringContext(a, "b\\t").s(1))
    println(made().s(2))
    println({ println("block"); StringContext }.apply("[", "]").s(3))
    println(raw(StringContext("\\n", "|"), 4))
    try StringContext("p1", "p2").s() catch case e: IllegalArgumentException => println("args")

object Data1:
  def isEmpty: Boolean = false
  def get: Data2.type = Data2
object Data2:
  def apply(i: Int): Int = i * 10
  def drop(n: Int): Seq[Int] = Seq(n)
  def toSeq: Seq[Int] = Seq(6, 7)
  def lengthCompare(len: Int): Int = 0

object Ints:
  def unapplySeq(x: Array[Int]): Data1.type = Data1
object Words:
  def unapplySeq(s: String): Option[Seq[String]] = if s.isEmpty then None else Some(s.split(" ").toList)

object Extractors:
  def first(xs: Array[Int]): Int = xs match { case Ints(x, y) => x + y }
  def rest(xs: Array[Int]): Seq[Int] = xs match { case Ints(x, ys*) => ys }
  def words(s: String): String = s match
    case Words(a, b) => b + a
    case Words(a, rest*) => a + rest.length
    case _ => "none"
  def run(): Unit =
    println(first(Array(1)))
    println(rest(Array(1)))
    println(words("x y"))
    println(words("x y z"))
    println(words(""))

case class Multi(x: Int)(val y: Int)(z: String)

object Calls:
  class R:
    def m_:(f: => Int): Int = f + 1
  def run(): Unit =
    val m = Multi(1)(2)("z")
    println(m)
    println(m.y)
    println(m match { case Multi(x) => x })
    val seq: Seq[String] = List("one", "two")
    println(java.util.Arrays.asList(seq*))
    val r = new R
    println({ println("lhs"); 4 } m_: r)

object Returns:
  def wrap[K](body: => K): K = body
  def f(k: Int): Option[Int] =
    type T = Option[Int]
    val t: T = wrap { if k > 2 then return Some(k); None }
    t

object Reduced:
  def anon(): Unit = (new Function0[Unit] { def apply() = println("anon") })()
  def two(k: Int): Unit = ((x: Int, y: String) => println(y + x))(k, "k" + k)
  def folded(): Int = ((x: Int) => x * 3)(2)
  def run(): Unit =
    anon()
    two(5)
    println(folded())

object Statement:
  val x: Option[String] = Some("stmt")
  x match { case Some(a) => println(a) case None => () }

@main def exprforms2(): Unit =
  Seqs.run()
  Branches.run()
  Interpolations.run()
  Extractors.run()
  Calls.run()
  println(Returns.f(1))
  println(Returns.f(5))
  Reduced.run()
  Statement
