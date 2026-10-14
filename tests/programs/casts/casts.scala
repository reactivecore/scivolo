//> using scala 3.9.0

class Store:
  def get[X](key: String): X = null.asInstanceOf[X]

def name(i: Int) = s"n$i"

@main def casts =
  val u = null.asInstanceOf[Unit]
  println(u)
  val v = name(1).asInstanceOf[Unit]
  println(v)
  println(().asInstanceOf[AnyRef & Unit].getClass)
  println(7.asInstanceOf[AnyRef & Int].getClass)
  println((7: Any).##)
  println(null.asInstanceOf[Int] + null.asInstanceOf[Long])
  val store = Store()
  try
    "key" match
      case k: String => store.get(k)
  catch case _: NullPointerException => println("npe")
