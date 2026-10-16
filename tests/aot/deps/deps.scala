//> using dep com.lihaoyi::geny::1.1.0

@main def deps(): Unit =
  geny.Generator.from(List(1, 2, 3, 4)).filter(_ % 2 == 0).map(_ * 10).foreach(println)
