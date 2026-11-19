object ImplicitBestFirstWithTypeVariables {
  trait Read[A]
  object Read extends ReadImplicits {
    implicit val unit: Read[Unit] = ???
  }
  trait ReadImplicits {
    given roe: Read[Option[EmptyTuple]] = ???
    given rou: Read[Option[Unit]] = ???
    given cons1: [H, T <: Tuple] => (Read[Option[H]], Read[Option[T]]) => Read[Option[H *: T]] = ???
  }

  trait Fragment {
    def query[B: Read]: String = ???
  }

  class X(val value: Int)
  class Y(val x: X)
  class Z(val y: Y)
  trait Conv[A, B] extends Conversion[A, B]
  given xy: Conv[X, Y] = { (x: X) => new Y(x) }
  given yz: Conv[Y, Z] = { (y: Y) => new Z(y) }
  object ConvUtils {
    implicit def syllogism[A, B, C](using ab: Conv[A, B], bc: Conv[B, C]): Conv[A, C] =
      new Conv[A, C] { def apply(a: A): C = bc(ab(a)) }
  }
  import ConvUtils.syllogism

  def test(f: Fragment): Unit = {
    f.query
    val z: Z = new X(42)
  }
}
