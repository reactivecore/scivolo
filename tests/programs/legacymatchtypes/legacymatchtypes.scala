//> using options -source:3.3

case class Wrap[T](t: T)
class Dummy

trait Show[A] { def show: String }
object Show {
  given Show[Int] with { def show = "Int" }
  given Show[String] with { def show = "String" }
}

trait Functor[F[_]] { def name: String }
object Functor {
  given Functor[List] with { def name = "List" }
  given Functor[Option] with { def name = "Option" }
}

object legacymatchtypes {
  type Apply[T[_]] = T[Dummy]
  type Id[T] = T
  type Unapply[F[_[_]], T] = T match {
    case Wrap[Apply[a]] => F[a]
    case Wrap[Dummy] => F[Id]
  }

  type Elem[X] = X match {
    case List[t] => t
    case Option[t] => t
  }

  type Second[X <: Tuple] = X match {
    case a *: b *: _ => b
  }

  type ArrayElem[X] = X match {
    case Array[t] => t
  }

  def show[A](using s: Show[A]): String = s.show

  def first[T <: Array[Int]](a: T): ArrayElem[T] = a(0)

  def main(args: Array[String]): Unit = {
    println(summon[Unapply[Functor, Wrap[List[Dummy]]]].name)
    println(summon[Unapply[Functor, Wrap[Option[Dummy]]]].name)
    println(show[Elem[List[Int]]])
    println(show[Elem[Option[String]]])
    println(show[Second[(Int, String, Boolean)]])
    val s: Second[(Int, String)] = "second"
    println(s)
    println(first(Array(7, 8)) + 1)
  }
}
