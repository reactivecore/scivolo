type Person = (name: String, age: Int)
type Address = (city: String, zip: Int)

val bob: Person = (name = "Bob", age = 33)
val addr = (city = "Lausanne", zip = 1003)

case class City(name: String, pop: Int)

def ageOf(p: Person) = p.age
def first(t: (String, Int)) = t._1

@main def namedtupleops =
  val t: (String, Int) = bob
  println(t)
  println(bob.toTuple)
  println(first(bob))
  println(bob(0))
  println(ageOf(("anon", 11)))
  val both = bob ++ addr
  println(both)
  println(both.city)
  println(bob.size)
  println(bob.head)
  println(bob.tail)
  println(bob.reverse)
  println(bob.toList)
  println(bob.toSeqMap)
  val nt: NamedTuple.NamedTuple[("a", "b"), (Int, String)] = (42, "Hello")
  println(nt.a)
  println(NamedTuple.build[("a", "b")]()((1, "x")).b)
  println(bob == (name = "Bob", age = 33))
  val m = summon[deriving.Mirror.Of[Person]]
  println(m.fromProduct(("Ann", 7)).name)
  val city: NamedTuple.From[City] = (name = "Lausanne", pop = 140000)
  println(city.pop)
