//> using scala 3.9.0
class A:
  def f(x: Int): Int = x + 1
  override def toString = "A"

trait T extends A:
  override def f(x: Int): Int = super.f(x) * 2

class B extends A with T:
  override def f(x: Int): Int = super.f(x) + 100
  def g = super[T].f(1) + super[A].f(1)
  override def toString = super.toString + "B"

abstract class Base:
  def name: String = "base"
  def tag(n: Int): String = s"base$n"

class Impl extends Base:
  override def name = "impl"

trait Loud extends Base:
  override def name = super.name.toUpperCase
  override def tag(n: Int) = "loud:" + super.tag(n)

trait Twice extends Base:
  override def name = super.name * 2
  override def toString = "twice:" + name

class Both extends Impl with Loud with Twice
class Other extends Impl with Twice with Loud

trait Box[V]:
  def put(a: V): String = s"box($a)"

trait Logged[V] extends Box[V]:
  override def put(a: V): String = "log:" + super.put(a)

class IntBox extends Box[Int] with Logged[Int]

abstract class Q:
  def m(x: Int): Int

class QImpl extends Q:
  def m(x: Int) = x + 1

trait Stack extends Q:
  abstract override def m(x: Int) = super.m(x) * 10

trait Stack2 extends Q:
  abstract override def m(x: Int) = super.m(x) - 1

class All extends QImpl with Stack with Stack2

trait Calls:
  def twice(f: Int => Int) = f(f(1))

trait Calls2 extends Calls:
  override def twice(f: Int => Int) = super.twice(f) + super.twice(x => x)

class C2 extends Calls2

trait M:
  def h(): String = "M"

class P:
  def h(): String = "P"

class Outer extends P with M:
  override def h() = "Outer"
  class Inner:
    def c() = Outer.super[M].h() + Outer.super[P].h() + Outer.super.h()
  def loop() = for i <- 0 until 1 do println(super[M].h() + i)

trait N extends M:
  override def h() = "N"
  class InN:
    def d() = N.super.h() + N.super[M].h()

class D extends N

@main def supertraits(): Unit =
  val b = new B
  println(b.f(1))
  println(b.g)
  println(b)
  println(new Both().name)
  println(new Other().name)
  println(new Both().tag(3))
  println(new Other())
  println(new IntBox().put(4))
  println(new All().m(1))
  println(new C2().twice(_ + 1))
  val o = new Outer
  println(new o.Inner().c())
  o.loop()
  val d = new D
  println(new d.InN().d())
