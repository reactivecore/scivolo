def byName[T](p: => T): T = p
val test = (if ??? then byName else (??? : ((=> Int) => Int))) (42)
val f = (if ??? then (x: Any) => 1 else (x: Int) => "s")
val r = f(3)
