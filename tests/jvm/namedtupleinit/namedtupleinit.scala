//> using options -source future
type City = (name: String, zip: Int, pop: Int)

object NamedTupleInit:
  def main(args: Array[String]): Unit =
    val city: City = (name = "Lausanne", zip = 1000, pop = 140000)
    val cinit = city.init
    println(cinit.name)
    println(cinit.zip)
