// The instances implicit searches choose for types with unbounded type variables, through the
// implicit scope of a type variable's bound and of bounded type lambdas.

object Searches {
  trait Show[A] { def name: String }
  object Show extends ShowLow {
    given showList[A](using s: Show[A]): Show[List[A]] with { def name = "list(" + s.name + ")" }
    given showInt: Show[Int] with { def name = "int" }
  }
  trait ShowLow {
    given showAny[A]: Show[A] with { def name = "any" }
  }
  def pick[A](using s: Show[A]): String = s.name

  trait Tc[A] { def name: String }
  object Tc extends TcLow {
    given optList[A](using inner: Tc[List[A]]): Tc[Option[A]] with { def name = "optList(" + inner.name + ")" }
  }
  trait TcLow {
    given tcAny[A]: Tc[A] with { def name = "tcAny" }
  }
  def pickTc[A](using t: Tc[A]): String = t.name

  trait Read[A] { def name: String }
  object Read extends ReadLow {
    given unit: Read[Unit] with { def name = "unit" }
  }
  trait ReadLow {
    given roe: Read[Option[EmptyTuple]] with { def name = "roe" }
    given cons[H, T <: Tuple](using h: Read[Option[H]], t: Read[Option[T]]): Read[Option[H *: T]] with {
      def name = "cons(" + h.name + "," + t.name + ")"
    }
  }
  def query[B](using r: Read[B]): String = r.name

  trait Ord[-A] { def name: String }
  object Ord {
    given ordAny: Ord[Any] with { def name = "ordAny" }
    given ordString: Ord[String] with { def name = "ordString" }
  }
  def pickOrd[A](using o: Ord[A]): String = o.name

  trait Box[+A] { def name: String }
  object Box {
    given boxNothing: Box[Nothing] with { def name = "boxNothing" }
    given boxString: Box[String] with { def name = "boxString" }
  }
  def pickBox[A](using b: Box[A]): String = b.name

  trait Sem[A] { def name: String }
  object Sem {
    given semPair[A, B](using a: Sem[A], b: Sem[B]): Sem[(A, B)] with { def name = "pair(" + a.name + "," + b.name + ")" }
    given semString: Sem[String] with { def name = "string" }
    given semTop[A <: AnyRef]: Sem[A] with { def name = "top" }
  }
  def pickSem[A](using s: Sem[A]): String = s.name

  class X(val value: Int)
  class Y(val x: X)
  class Z(val y: Y)
  trait Conv[A, B] extends Conversion[A, B] { def name: String }
  given xy: Conv[X, Y] with { def apply(x: X) = new Y(x); def name = "xy" }
  given yz: Conv[Y, Z] with { def apply(y: Y) = new Z(y); def name = "yz" }
  object ConvUtils {
    implicit def syllogism[A, B, C](using ab: Conv[A, B], bc: Conv[B, C]): Conv[A, C] =
      new Conv[A, C] { def apply(a: A): C = bc(ab(a)); def name = "syl(" + ab.name + "," + bc.name + ")" }
  }
  import ConvUtils.syllogism
  def conv[A, B](using c: Conv[A, B]): String = c.name

  def run(): Unit = {
    println(pick)
    println(pick[List[Int]])
    println(pick[List[String]])
    println(pickTc)
    println(pickTc[Option[Int]])
    println(query)
    println(pickOrd)
    println(pickBox)
    println(pickSem)
    println(pickSem[(String, String)])
    println(conv[X, Z])
    val z: Z = new X(42)
    println(z.y.x.value)
  }
}

object Merges {
  sealed abstract class HList {
    type Merge[L <: HList] <: HList
    def merge[L <: HList](l: L): Merge[L]
  }
  final case class HCons[H, T <: HList](head: H, tail: T) extends HList {
    type Merge[L <: HList] = HCons[H, tail.Merge[L]]
    def merge[L <: HList](l: L): Merge[L] = HCons(head, tail.merge(l))
  }
  sealed trait HNil extends HList {
    type Merge[L <: HList] = L
    def merge[L <: HList](l: L): Merge[L] = l
  }
  final val HNil: HNil = { case object HNil extends HNil; HNil }

  def run(): Unit = {
    val merged: HCons[Int, HCons[String, HNil]] = HCons(42, HNil).merge(HCons("foo", HNil))
    println(merged.head)
    println(merged.tail.head)
    println(merged.tail.tail eq HNil)
    println(HNil.asInstanceOf[Product].productPrefix)
  }
}

object Lengths {
  sealed trait Nat
  case class Succ[N <: Nat](n: N) extends Nat
  case object Zero extends Nat
  type Zero = Zero.type
  type One = Succ[Zero]

  sealed trait HList
  case class HCons[+H, +T <: HList](head: H, tail: T) extends HList
  case object HNil extends HList
  type HNil = HNil.type

  trait Length[L <: HList] {
    type Out <: Nat
    def name: String
  }
  object Length {
    type Aux[L <: HList, Out0 <: Nat] = Length[L] { type Out = Out0 }
    def instance[L <: HList, Out0 <: Nat](n: String): Aux[L, Out0] = new Length[L] { type Out = Out0; def name = n }

    given hnilLength: Aux[HNil, Zero] = instance("hnil")
    given hconsLength: [H, T <: HList] => (length: Length[T]) => Aux[HCons[H, T], Succ[length.Out]] =
      instance("hcons(" + length.name + ")")
  }

  def run(): Unit = {
    val one = summon[Length.Aux[HCons[Int, HNil], One]]
    println(one.name)
    println(summon[Length.Aux[HCons[Int, HCons[String, HNil]], Succ[One]]].name)
  }
}

object implicitchoice {
  def main(args: Array[String]): Unit = {
    Searches.run()
    Merges.run()
    Lengths.run()
  }
}
