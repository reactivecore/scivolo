//> using scala 3.9.0
trait Nt24Functor[F[_]] { def name: String }

trait Nt24Shape { type Elems[_] }

object Nt24hkargs {
  given Nt24Functor[List] with { def name = "list" }

  def kinded[A <: AnyKind](n: Int): Int = n + 1

  def functorOf(s: Nt24Shape { type Elems[X] = List[X] })(using f: Nt24Functor[s.Elems]): String = f.name

  def main(args: Array[String]): Unit = {
    println(kinded[List](1))
    println(kinded[Int](2))
  }
}
