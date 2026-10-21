//> using dep com.lihaoyi::ujson::4.4.3

@main def run(): Unit =
  val j = ujson.read("""{"a":[1,2,{"b":null}],"s":"x"}""")
  println(j("a")(1).num)
  println(j("s").str)
  val o = ujson.Obj("k" -> ujson.Num(1), "l" -> ujson.Arr(1, 2, 3))
  o("m") = "added"
  println(ujson.write(o))
  println(ujson.write(j, indent = 2))
