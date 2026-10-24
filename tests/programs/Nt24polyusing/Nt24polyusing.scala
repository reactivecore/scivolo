//> using scala 3.9.0
trait Nt24Zero[F[_]] {
  def name: String
}

object Nt24polyusing {
  given Nt24Zero[List] with { def name = "list" }
  given Nt24Zero[Option] with { def name = "option" }

  def main(args: Array[String]): Unit = {
    val test = [F[_]] => (f: Nt24Zero[F]) ?=> [G[_]] => (g: Nt24Zero[G]) ?=> f.name + " " + g.name
    println(test[List][Option])
  }
}
