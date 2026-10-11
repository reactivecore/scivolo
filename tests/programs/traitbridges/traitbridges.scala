//> using scala 3.9.0
class B:
  override def toString = "B"
class S extends B:
  override def toString = "S"

trait T:
  def f: B = new B()
trait U extends T:
  override def f: S = new S()
class C extends U

trait Define[A]:
  protected def coll: Define[A]
  def s = coll
trait Iterable[A] extends Define[A]:
  protected def coll: this.type = this
trait Seq[A] extends Iterable[A]
trait Super1[A]:
  protected def coll: Iterable[A]
trait Super2[A] extends Super1[A]:
  override protected def coll: Seq[A]
class Foo[T] extends Seq[T] with Super2[T]

trait Named:
  override def clone(): String = "named"
class Cloned extends Named with Cloneable:
  def viaNamed = (this: Named).clone()

@main def traitbridges(): Unit =
  val c = new C
  println(c.f)
  println((c: T).f)
  println(new Foo[Int] != null)
  println(new Cloned().viaNamed)
