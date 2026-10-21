class Person(val name: String, val age: Int)

object Person:
  def unapply(p: Person): (name: String, age: Int) = (name = p.name, age = p.age)

object Adult:
  def unapply(p: Person): Option[(name: String, age: Int)] =
    if p.age >= 18 then Some((name = p.name, age = p.age)) else None

case class City(name: String, zip: Int)

def describe(p: (name: String, home: City)): String = p match
  case (home = City(zip = 1003), name = n) => s"$n lives in 1003"
  case (name = n) => s"$n lives elsewhere"

@main def namedpatterns =
  val (x = a, y = b) = (x = 1, y = 2)
  println(a + b)
  val (u, v) = (x = 3, y = 4)
  println(u * v)

  (left = "l", right = "r") match
    case (right = r, left = l) => println(s"$l$r")

  (left = "l", right = "r") match
    case (l, r) => println(s"$r$l")

  (outer = (inner = 7, other = 8), flag = true) match
    case (outer = (other = o), flag = false) => println(s"no $o")
    case (outer = (other = o, inner = i)) => println(s"nested $i $o")

  Some((name = "Bob")) match
    case Some(name = n) => println(n)

  Person("Ann", 30) match
    case Person(age = g) => println(g)
  Person("Ann", 30) match
    case Person(n, g) => println(s"$n $g")

  for p <- List(Person("Cid", 12), Person("Dee", 40)) do
    p match
      case Adult(name = n) => println(s"adult $n")
      case _ => println("minor")

  println(describe((name = "Eve", home = City("Lausanne", 1003))))
  println(describe((name = "Fay", home = City("Bern", 3000))))
