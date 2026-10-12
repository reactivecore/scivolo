//> using scala 3.9.0
inline def top(x: Int): Int = x * 2
inline def inline1(inline f: Int => Int): Int => Int = i => f(1)
inline def inline2(inline f: Int => Int): Int = f(2) + 3

object Obj:
  inline def sq(x: Int): Int = x * x

object Holder:
  println("init Holder")
  val v = 1

trait T:
  inline def tw: Int = 2

class C(val n: Int):
  def base: Int = n * 10
  inline def plus(m: Int): Int = n + m
  inline def twice: Int = base + base
  inline def self: C = this
  def useInside(k: Int): Int = plus(k)

class D(val k: Int) extends T:
  val f: Int = k + 1
  inline def both(x: Int): Int = f + k + x
  inline def unused: Int = 3
  def inner: Int = both(1) + this.both(2) + tw

case class P(a: Int, b: Int):
  inline def sum: Int = a + b

object Calls:
  inline def twice(x: Int): Int = x + x
  inline def pick(inline c: Boolean, a: Int, b: Int): Int = if c then a else b
  inline def byName(x: => Int): Int = x + x
  inline def gen[A](x: A): A = x
  inline def two(a: Int)(b: Int): Int = a - b
  inline def local(x: Int): Int = { val y = x + 1; y * y }
  inline def choose(inline flag: Boolean, a: Int): Int = if flag then a else 0
  inline def ignore(x: Int): Int = 7
  inline def greet(name: String): String = "hi " + name
  inline def unit(x: Int): Unit = println(x)
  inline def label(x: Int, inline g: Int => String): String = g(x)
  def side(): Int = { println("side"); 1 }
  def f: Int => String = _.toString
  def run(args: Array[String]): Unit =
    val n = args.length
    println(twice(21))
    println(twice(side()))
    println(pick(n == 0, 1, side()))
    println(top(n))
    println(Obj.sq(3))
    println(Obj.sq(n))
    println(byName(side()))
    println(gen[String]("s"))
    println(gen(n))
    println(two(n)(side()))
    println(local(n))
    println(choose(true, n))
    println(choose(false, side()))
    println(ignore(side()))
    println(ignore(n))
    println(greet("bob"))
    println(greet(args.mkString))
    println(label(n, f))
    unit(n)

object Nested:
  inline def inc(x: Int): Int = x + 1
  inline def inc2(x: Int): Int = inc(inc(x))
  inline def sq(x: Int): Int = x * x
  inline def sqInc(x: Int): Int = sq(inc(x))
  inline def fact(n: Int): Int = if n == 0 then 1 else n * fact(n - 1)
  inline def app(f: Int => Int, x: Int): Int = f(x)
  inline def appI(inline f: Int => Int, x: Int): Int = f(x) + f(x)
  inline def twice(x: => Int): Int = x + x
  def side(): Int = { println("side"); 2 }
  def test: Int = inline2(inline1(2.+))
  def run(args: Array[String]): Unit =
    val n = args.length
    println(inc2(n))
    println(inc2(3))
    println(sqInc(side()))
    println(fact(3))
    println(app(_ + 1, n))
    println(app(y => { val z = y * 2; z + 1 }, 3))
    println(appI(_ * 3, n))
    println(twice({ val q = n + 1; q }))
    println(test)

object Arguments:
  final val K = 5
  val w = 2
  lazy val lz: Int = { println("lz"); 3 }
  inline def ib(inline x: => Int): Int = x + x
  inline def ib0(inline x: => Int): Int = 0
  inline def twice(x: Int): Int = x + x
  inline def unused(x: Int): Int = 1
  inline def sub(a: Int, b: Int): Int = a - b
  inline def p(inline x: Int): Int = x * 2
  inline def q(inline b: Boolean): String = if b then "y" else "n"
  def s(i: Int): Int = { println(i); i }
  def run(args: Array[String]): Unit =
    lazy val loc: Int = { println("loc"); 4 }
    println(ib(s(1)))
    println(ib0(s(2)))
    println(unused(lz))
    println(unused(loc))
    println(unused(Holder.v))
    println(twice(Holder.v))
    println(twice(w))
    println(sub(b = s(1), a = s(2)))
    println(p(3 + 4))
    println(p(K))
    println(q(K > 3))
    println(q(1 == 1))
    println(p(args.length))

object Classes:
  inline def inc(x: Int): Int = x + 1
  def mk(): C = { println("mk"); C(1) }
  def mkD(): D = { println("mkD"); D(1) }
  def run(): Unit =
    val c = C(2)
    println(c.plus(3))
    println(mk().plus(4))
    println(c.twice)
    println(mk().self.n)
    println(c.useInside(5))
    val d = D(4)
    println(d.both(5))
    println(mkD().unused)
    println(d.tw)
    println(P(1, 2).sum)
    val p = P(3, 4)
    println(p.sum)
    println(d.inner)
    println(inc(c.n))

@main def inlining(): Unit =
  val args = Array.empty[String]
  Calls.run(args)
  Nested.run(args)
  Arguments.run(args)
  Classes.run()
