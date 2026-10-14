//> using scala 3.9.0
// Names of pattern-matching variables: call scrutinees, nested case classes, alternatives.

object Calls {
  def a(): Option[String] = Some("a")
  def plain: Any = a() match {
    case Some(b1) => b1
    case None => None
  }
  def used: Any = a() match {
    case Some(b1) => b1 + b1
    case None => None
  }
  def inner: Any = a() match {
    case Some(b1) => a() match {
      case Some(b2) => b2
      case None => None
    }
    case None => None
  }
}

object O {
  def f(x: Any): Int = x match {
    case i: Int => i
    case _ => 0
  }
  case class P(a: Int, b: String)
  def h(p: Any): Int = p match {
    case P(a, _) => a
    case Q(n) => n
    case _ => 0
  }
  case class Q(n: Int)
  def g(x: Any): String = x match {
    case y @ (1 | 2) => "small " + y
    case s @ ("a" | "b") => s.toString
    case Some(z @ (3 | 4)) => "some " + z
    case _ => "other"
  }
}

object Test {
  def run(): Unit = {
    println(Calls.plain)
    println(Calls.used)
    println(Calls.inner)
    println(O.f(1))
    println(O.P(1, "x") == O.P(1, "x"))
    println(O.h(O.Q(4)))
    println(O.g(2))
    println(O.g("b"))
    println(O.g(Some(3)))
  }
}

@main def matchnames(): Unit = Test.run()
