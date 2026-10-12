//> using scala 3.9.0
import scala.language.experimental.subCases

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
  inline def unitBlock(x: Int): Unit = { val y = x + 1; println(y) }
  inline def unitIf(x: Int): Unit = if x > 0 then println(x)
  inline def withLambda(a: Int)(b: Int): Int = ((x: Int) => x + b)(a)
  def side(): Int = { println("side"); 1 }
  def lam: Int = withLambda(1)(2)
  inline def adder(x: Int): Int => Int = (a: Int) => a + x
  def lam2: Int = adder(1)(2)
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
    unitBlock(n)
    unitIf(n)
    unitIf(side())
    println(withLambda(1)(2))
    println(lam)
    println(lam2)
    println(adder(n)(3))
    println(adder(side())(n))
    println(withLambda(n)(side()))

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

transparent inline def topT(n: Int): Any = if n > 0 then "pos" else n

object Transparent:
  transparent inline def choose(b: Boolean): Any = if b then 1 else "one"
  transparent inline def outer(b: Boolean): Any = choose(!b)
  transparent inline def viaPlain(x: Int): Any = Nested.inc(x) * 2
  inline def callsTransparent(b: Boolean): Any = choose(b)
  transparent inline def lazily(x: => Int): Any = x + x
  transparent inline def pick(inline c: Boolean, a: Int, b: String): Any = if c then a else b
  extension (x: Int) transparent inline def double: Any = x * 2
  transparent inline def first[T](xs: List[T]): Option[T] = xs.headOption
  transparent inline def answer: Any = 42
  def over(i: Int): String = "int"
  def over(s: String): String = "str"
  class Counter(var n: Int):
    transparent inline def next(): Any = { n += 1; n }
  def side(): Int = { println("side"); 2 }
  def run(): Unit =
    val a = outer(false)
    println(a + 1)
    val b = viaPlain(side())
    println(b + 1)
    println(callsTransparent(true))
    println(lazily(side()))
    println(pick(b = "x", a = side(), c = false).length)
    println(pick(true, side(), "y") + 1)
    println(3.double + 1)
    println(first(List(1, 2)).isEmpty)
    println(over(choose(true)))
    println(over(choose(false)))
    println(Counter(4).next() + 1)
    println(topT(1).length)
    val l: Int = answer
    println(later(l) + 1)
    println(dflt() + 1)
    println(apT(_ => 5))
  transparent inline def later(x: Int): Any = x
  transparent inline def dflt(x: Int = 3): Any = x
  transparent inline def apT(inline f: Any => Int): Any = f(N4)

trait Noisy:
  println("Noisy init")

object N1 extends Noisy
object N2 extends Noisy
object N3 extends Noisy:
  val u: Unit = ()
object N4 extends Noisy

inline def ap(inline f: Any => Int): Int = f(N1)

object Library:
  given Int = 7
  def run(args: Array[String]): Unit =
    val n = args.length
    assert(n == 0)
    assert(n >= 0, "negative")
    println(summon[Int] + n)
    try assert(n > 0, s"n is $n")
    catch case e: AssertionError => println(e.getMessage)
    try assert(n != 0)
    catch case e: AssertionError => println(e.getMessage)
    val ev = summon[Int <:< Any]
    println(ev != null)
    println(summon[String =:= String].apply("ev"))

object Matches:
  inline def kind(x: Int): String = x match
    case 0 => "zero"
    case 1 | 2 => "small"
    case n if n < 0 => "neg " + n
    case _ => "big"
  inline def safeDiv(x: Int): Int =
    try 10 / x
    catch case e: ArithmeticException => -1
  inline def orElse(o: Option[Int]): Int = o match
    case Some(v) => v
    case None => -1
  def run(args: Array[String]): Unit =
    val n = args.length
    println(kind(n))
    println(kind(-3))
    println(safeDiv(n))
    println(safeDiv(5))
    println(orElse(Some(n)))
    println(orElse(None))

case class IPair(a: Int, b: String)
enum IColor:
  case Red, Green

object InlineMatches:
  inline def sign(inline n: Int): String = inline if n > 0 then "pos" else "nonpos"
  inline def lit(inline n: Int): Int = inline n match
    case 0 => 10
    case 1 | 2 => 20
    case k if k > 5 => k * 2
    case _ => 0
  inline def fact(inline n: Int): Int = inline n match
    case 0 => 1
    case _ => n * fact(n - 1)
  inline def first(p: IPair): Int = inline p match
    case IPair(a, _) => a + 1
  inline def opt(o: Option[Int]): Int = inline o match
    case Some(x) => x
    case None => -1
  inline def col(c: IColor): String = inline c match
    case IColor.Red => "red"
    case IColor.Green => "green"
  inline def name[T]: String = inline compiletime.erasedValue[T] match
    case _: Int => "int"
    case _: String => "string"
    case _ => "other"
  inline def sub(o: Option[Int]): String = inline o match
    case Some(x) if x match
      case 1 => "one"
      case _ => "many"
    case _ => "none"
  def run(args: Array[String]): Unit =
    println(sub(Some(args.length)))
    println(sub(None))
    println(sign(3))
    println(sign(-3))
    println(lit(2) + lit(7) + lit(4))
    println(fact(5))
    println(first(IPair(args.length, "x")))
    println(opt(Some(args.length)))
    println(opt(None))
    println(col(IColor.Green))
    println(name[Int] + name[String] + name[Boolean])

trait IShow[T]:
  def show(t: T): String

object IShow:
  given IShow[Int] = new IShow[Int] { def show(t: Int) = "int " + t }

object CompileTime:
  import compiletime.{codeOf, constValue, constValueOpt, requireConst, summonInline}
  import compiletime.ops.int.+
  inline def plusOne[N <: Int]: Int = constValue[N] + 1
  inline def opt[T]: Option[T] = constValueOpt[T]
  inline def check(inline x: Int): Int =
    requireConst(x)
    x
  inline def show[T](t: T): String = summonInline[IShow[T]].show(t)
  inline def sum[A <: Int, B <: Int]: Int = constValue[A + B]
  inline def describe[T](t: T): String = compiletime.summonFrom:
    case s: IShow[T] => s.show(t)
    case _ => "no show"
  inline def has[T]: Boolean = compiletime.summonFrom:
    case given IShow[T] => true
    case _ => false
  def run(args: Array[String]): Unit =
    println(constValue[3] + plusOne[4])
    println(constValue[1 + 2])
    println(opt[5])
    println(check(3))
    println(show(args.length))
    println(summonInline[IShow[Int]].show(2))
    println(sum[2, 5])
    println(codeOf(7))
    println(describe(args.length) + " " + describe("s"))
    println(has[Int] && !has[String])

@main def inlining(): Unit =
  println(ap(_ => 3))
  println(((_: Any) => 4)(N2))
  ((u: Unit) => println("body"))(N3.u)
  val args = Array.empty[String]
  Calls.run(args)
  Nested.run(args)
  Arguments.run(args)
  Classes.run()
  Transparent.run()
  Library.run(args)
  Matches.run(args)
  InlineMatches.run(args)
  CompileTime.run(args)
