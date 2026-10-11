//> using scala 3.9.0
object Init:
  println("init")
  val x: Int = 1

object Plain:
  val y: Int = 2

object Lazy:
  lazy val z: Int = 3

case class P(a: Int, b: Int)

case class Q(a: Int)
object Q:
  println("q")

case class R(a: Int):
  println("r")

class Base(x: Int)
case class S(a: Int) extends Base(a)

class HasVal(val a: Int)
object OB extends HasVal(1)
object OC extends HasVal(1):
  val b: Int = 2

object Sel:
  def next(): Int = 1
  def fromR(): (Int, Int) = (R(1).a, 2)
  def fromS(): (Int, Int) = (S(1).a, 2)
  def fromQ(): (Int, Int) = (Q(1).a, 2)
  def fromOB(): (Int, Int) = (OB.a, 1)
  def fromOC(): (Int, Int) = (OC.b, 1)
  def fromInit(): (Int, Int) = (Init.x, 1)
  def fromCase(p: P): (Int, Int) = (p.a, p.b)
  def swapped(t: (Int, Int)): (Int, Int) = (t._1, t._2)
  def mkQ(): (Int, Q) = (1, Q(2))
  def generic[T](x: T): T = x
  def viaGeneric(): Any = generic((1, 2))
  def swap(t: (Int, Int)): (Int, Int) = t.swap
  def copy(t: (Int, Int)): (Int, Int) = t.copy(_1 = 5)
  def unit(): (Unit, Int) = ((), 1)
  def lazyv(): (Int, Int) =
    lazy val z = 1
    (z, 2)
  def varv(): (Int, Int) =
    var v = 1
    (v, 2)
  def block(): (Int, Int) = ({ val c = new Object; 1 }, 2)
  def nested(): ((Int, Int), Int) = ((1, 2), 3)
  def nestedSel(t: ((Int, Int), Int)): Int = t._1._1
  val lit: (1, 2) = (1, 2)
  def sel(p: (1, 2)): Int = p._1
  def ident[T <: Int](x: T): (T, T) = (x, x)
  def selb[T <: Int](p: (T, T)): T = p._1
  type I = Int
  val al: (I, I) = (1, 2)
  def viaCall(): Any = next().getClass
  def viaVar(): Any =
    var v = 1
    v.getClass
  def viaTypedVar(): Any =
    var v = 1
    (v: Int).getClass
  def viaLazy(): Any = Lazy.z.getClass
  def viaSum(a: Int, b: Int): Any = (a + b).getClass
  def name(x: Any): String = x.getClass.getName

@main def tuplespec(): Unit =
  val a = 3
  val b = 4
  val t = (a, b)
  println(t._1 + t._2)
  val any: Any = (1, 2)
  println(any.getClass.getName)
  println((1L, 2.0).getClass.getName)
  println(('c', true).getClass.getName)
  println(Tuple1(5).getClass.getName)
  println(("s", 1).getClass.getName)
  println((a + b, a).getClass.getName)
  println(new Tuple2(a, 1.5).getClass.getName)
  val u = (2L, 'x')
  println(u._2)
  t match
    case (x, y) => println(x * y)
  println(Init.x.getClass)
  println(Plain.y.getClass)
  println((5: Int).getClass)
  println(P(1, 2).a)
  println(Sel.name(Sel.fromR()))
  println(Sel.name(Sel.fromS()))
  println(Sel.name(Sel.fromQ()))
  println(Sel.name(Sel.fromOB()))
  println(Sel.name(Sel.fromOC()))
  println(Sel.name(Sel.fromInit()))
  println(Sel.name(Sel.fromCase(P(3, 4))))
  println(Sel.name(Sel.swapped(t)))
  println(Sel.name(Sel.mkQ()))
  println(Sel.name(Sel.viaGeneric()))
  println(Sel.name(Sel.swap(t)))
  println(Sel.name(Sel.copy(t)))
  println(Sel.name(Sel.unit()))
  println(Sel.name(Sel.lazyv()))
  println(Sel.name(Sel.varv()))
  println(Sel.name(Sel.block()))
  println(Sel.name(Sel.nested()))
  println(Sel.nestedSel(((7, 8), 9)))
  println(Sel.name(Sel.lit))
  println(Sel.sel(Sel.lit))
  println(Sel.name(Sel.ident(6)))
  println(Sel.selb((6, 7)))
  println(Sel.name(Sel.al))
  println(Sel.viaCall())
  println(Sel.viaVar())
  println(Sel.viaTypedVar())
  println(Sel.viaLazy())
  println(Sel.viaSum(1, 2))
