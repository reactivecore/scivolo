//> using scala 3.9.0
import scala.deriving.Mirror

object localcaptures:
  def enumInMethod() =
    enum E:
      case C1, C2
    E.values.mkString(",")

  def nestedCall() =
    object Bar:
      val x = 1
    object Foo:
      class C:
        def g = Bar.x
    new Foo.C().g

  def nestedCapture(k: Int) =
    def g = k
    class C:
      class D:
        def m = g
    val c = new C
    new c.D().m

  def mirrorOfLocal() =
    object Bar:
      case object A extends Foo.Item
    object Foo:
      sealed trait Item:
        final lazy val mItem = summon[Mirror.Of[Item.this.type]]
    Bar.A.mItem.ordinal(Bar.A)

  def localSum() =
    sealed trait Item
    object Wrapper:
      case object A extends Item
      case object B extends Item
    summon[Mirror.Of[Item]].ordinal(Wrapper.B)

  lazy val anon = { val x = 3; new AnyRef { override def toString = s"anon $x" } }

  def ordinal[T](x: List[T])(using m: Mirror.SumOf[List[T]]): Int = m.ordinal(x)

  def main(args: Array[String]): Unit =
    println(enumInMethod())
    println(nestedCall())
    println(nestedCapture(7))
    println(mirrorOfLocal())
    println(localSum())
    println(anon)
    println(s"${ordinal(List(1))} ${ordinal(Nil)}")
