trait CB[X] { def get: X }

trait WrapperConvert[F[_], G[_]]:
  def conv[X](fx: F[X]): G[X]

object WrapperConvert:
  implicit def id[F[_]]: WrapperConvert[F, F] = new WrapperConvert[F, F]:
    def conv[X](fx: F[X]): F[X] = fx

object Transparent:
  transparent inline given convertX: [F[_], X] => (wc: WrapperConvert[F, CB]) => Conversion[F[X], X] =
    new Conversion[F[X], X]:
      def apply(fx: F[X]) = wc.conv(fx).get

  def test(cb: CB[Int], x: Int): Int = cb + x

object Plain:
  given convertX: [F[_], X] => (wc: WrapperConvert[F, CB]) => Conversion[F[X], X] =
    new Conversion[F[X], X]:
      def apply(fx: F[X]) = wc.conv(fx).get

  def test(cb: CB[Int], x: Int): Int = (cb: Int) + x
