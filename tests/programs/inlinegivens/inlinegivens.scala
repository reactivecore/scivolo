trait Show[A] { def show(a: A): String }
object Show {
  given Show[Int] with { def show(a: Int) = s"int $a" }
  given [A](using s: Show[A]): Show[List[A]] with {
    def show(l: List[A]) = l.map(s.show).mkString("[", ",", "]")
  }
}

inline def sh[A](a: A): String = compiletime.summonInline[Show[A]].show(a)

object inlinegivens {
  def viaArg(): String = {
    given Show[Int] with { def show(a: Int) = s"local $a" }
    sh(List(1, 2))
  }

  def direct(): String = {
    given loc: Show[List[Int]] = new Show[List[Int]] { def show(l: List[Int]) = "local list" }
    sh(List(1))
  }

  def renamed(): String = {
    given li: Show[Int] = new Show[Int] { def show(a: Int) = s"li $a" }
    sh(1)
  }

  def main(args: Array[String]): Unit = {
    println(viaArg())
    println(direct())
    println(renamed())
    println(sh(List(3)))
  }
}
