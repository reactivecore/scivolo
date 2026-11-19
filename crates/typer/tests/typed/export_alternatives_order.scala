trait IOps[+A]:
  def concat[B >: A](other: IterableOnce[B]): Int = 1
  def concat(x: Int, y: Int): Int = 2
trait SOps[A] extends IOps[A]:
  def concat(other: IterableOnce[A]): String = "s"
class Test(s: SOps[Int]):
  export s.concat
class Test2(s: SOps[Int]):
  export s.*
