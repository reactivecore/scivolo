type Person = (name: String, age: Int)

val bob: Person = (name = "Bob", age = 33)
val n = bob.name
val (name = nm, age = ag) = bob

def describe(p: Person): String = p match
  case (name = n, age = a) if a > 18 => s"$n adult"
  case (name = n, age = _) => n

val xs = List(bob).map(p => (p.name, p.age))
val zipped: (a: Int, b: String) = (a = 1, b = "x")
