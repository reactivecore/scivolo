type Id[X] = X

@main def Lw26tuplemap(): Unit =
  val tuple: Tuple = ("1", "2", "3")
  val f: [t] => t => Id[t] = [t] => (x: t) => x
  println(tuple.map(f))
  val pair = (1, "a")
  println(pair.map[Option]([t] => (x: t) => Option(x)))
