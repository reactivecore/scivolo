import scala.compiletime.*

trait Show[T] { def show(t: T): String }
object Show {
  given Show[Int] = new Show[Int] { def show(t: Int) = "int " + t }
}

object Test {
  inline def show[T](t: T): String = summonFrom {
    case s: Show[T] => s.show(t)
    case _ => "no show"
  }
  inline def has[T]: Boolean = summonFrom {
    case given Show[T] => true
    case _ => false
  }
  inline def typed[T]: Boolean = summonFrom {
    case _: Show[T] => true
    case _ => false
  }
  inline def local[T](t: T): String = summonFrom {
    case s: Show[T] =>
      val u = t
      s.show(u)
    case _ => "no show"
  }
  inline def only[T](t: T): String = summonFrom {
    case s: Show[T] => s.show(t)
  }

  def main(args: Array[String]): Unit = {
    println(show(1))
    println(show("a"))
    println(has[Int])
    println(has[String])
    println(typed[Int])
    println(local(2))
    println(only(args.length))
  }
}
