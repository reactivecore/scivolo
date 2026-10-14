case class User(name: String, age: Int, city: String)

object Older {
  def unapply(u: User): Some[User] = Some(u)
}

object UserOf {
  def unapply(s: String): Some[User] = Some(User(s, 1, s))
}

case class Cell(var n: Int, tag: String)

object Main {
  def typed(x: Any): Int = x match {
    case Some(i: Int) => i
    case Some(s: String) => s.length
    case (a: Int, b: String) => a + b.length
    case _ => 0
  }
  def named(u: User): String = u match {
    case User(city = "Rome") => "roman"
    case User(age = a, name = n) => n + a
  }
  def nested(u: User): Int = u match {
    case Older(User(age = a)) => a
  }
  def viaGet(s: String): Int = s match {
    case UserOf(age = a, city = c) => a + c.length
  }
  def boxed(x: Any): Int = x match {
    case Some(i: Int) => i
    case Some(l: Long) => l.toInt
    case _ => 0
  }
  def cell(c: Cell): Int = c match {
    case Cell(n = k) => k
  }
  def pair(p: (Any, Any)): Any = p match {
    case t @ (_, l: List[?]) => t
    case _ => p
  }
  def run(): String = {
    val User(city = c) = User("a", 1, "b")
    named(User("x", 2, c)) + typed(Some(1)) + nested(User("y", 3, "z")) + pair((1, Nil))
  }
}
