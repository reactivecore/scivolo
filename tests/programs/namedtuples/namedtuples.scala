type Person = (name: String, age: Int)
val bob = (name = "Bob", age = 33): (name: String, age: Int)
val person2: (name: String, age: Int) = bob

type Uni = (uni: Double)
val uni = (uni = 1.0)
val _: Uni = uni

type AddressInfo = (city: String, zipCode: Int)
val addr = (city = "Lausanne", zipCode = 1003)
val _: AddressInfo = addr

def older(p: Person): Person = (name = p.name, age = p.age + 1)

@main def namedtuples =
  println(bob)
  println(bob.age)
  println(person2.name)
  println(uni.uni)
  println(s"${addr.city} ${addr.zipCode}")
  println(older(bob).age)
