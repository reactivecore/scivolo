//> using scala 3.9.0

class M(val x: Int) extends AnyVal:
  def adder: Int => Int = i => i + x
  def all(xs: List[Int]): List[M] = xs.map(i => new M(i + x))

class Meter(val v: Double) extends AnyVal:
  def twice: Meter = new Meter(v * 2)

class Name(val s: String) extends AnyVal
class Wrap[A](val a: A) extends AnyVal
class Arr(val a: Array[Int]) extends AnyVal

trait MySam[T]:
  def app(m: T): Any

trait Sam:
  def ap(m: M): M

trait UnitSam:
  def run(m: M): Unit

object valuelambdas:
  val sam: MySam[Meter] = (x: Meter) => (x: Any)

  def show(m: M): String = "M" + m.x
  def take(w: Wrap[Int]): Int = w.a + 1
  def first(o: Option[Wrap[Int]]): Int = take(o.get)

  def main(args: Array[String]): Unit =
    val m = new M(1)
    val f: M => Int = m => m.x + 1
    println(f(new M(1)))
    println(List(m).map(_.x))
    println(Option(m).map(n => n.x * 3))
    val g: Int => M = i => new M(i + 2)
    println(g(5).x)
    println(List(1, 2).map(new M(_)).map(_.x).sum)
    val h: M => M = n => new M(n.x * 10)
    println(h(m).x)
    val s: Sam = n => new M(n.x + 100)
    println(s.ap(m).x)
    val u: UnitSam = n => println(n.x)
    u.run(m)
    val k: M => Unit = n => println(n.x + 7)
    k(m)
    val two: (M, String) => String = (n, t) => t * n.x
    println(two(new M(3), "ab"))
    val t: String => M = str => new M(str.length)
    println(t("abcd").x)
    println(sam.app(new Meter(1.5)).asInstanceOf[Meter].v)
    val tw: Meter => Meter = _.twice
    println(tw(new Meter(2.0)).v)
    val c = 5
    val cap: M => Int = n => n.x + c
    println(cap(m))
    val f0 = () => m.x
    println(f0())
    val mk: () => M = () => new M(9)
    println(mk().x)
    List(m, new M(5)).foreach(n => println(n.x))
    val nm: Name => Int = n => n.s.length
    println(nm(new Name("abc")))
    val mkName: Int => Name = i => new Name("n" * i)
    println(mkName(3).s)
    val w: Wrap[Int] => Int = w => w.a + 1
    println(w(new Wrap(41)))
    val ws: String => Wrap[String] = s => new Wrap(s + "!")
    println(ws("hi").a)
    val pair: (Int, M) => M = (i, n) => new M(i + n.x)
    println(pair(1, m).x)
    println(List(m).map(show))
    println(m.adder(1))
    println(m.all(List(1, 2)).map(_.x))
    val id: M => M = n => n
    println(id(m).x)
    println(Option(m).map(n => new M(n.x + 1)).map(_.x))
    println(List(new Name("a"), new Name("bb")).map(_.s).mkString(","))
    println(first(Option(new Wrap[Int](6))))
    val af: Arr => Int = a => a.a.length
    println(af(new Arr(Array(1, 2, 3))))
    val an: Arr => Boolean = a => a.a == null
    println(an(null.asInstanceOf[Arr]))
