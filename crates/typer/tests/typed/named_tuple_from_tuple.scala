type Foo[T] = T
val x: NamedTuple.From[Tuple.Map[(Int, Int), Foo]] = ???
val res = x._1

type Z = NamedTuple.From[(Foo[Int], Foo[Int])]
val x2: Z = ???
val res2 = x2._2
