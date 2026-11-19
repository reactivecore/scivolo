//> using options -source:3.7

type Or[+A, +B] = A | B

val x: Or[Int, String] & Or[String, Int] = 3
val y: Or[Int & String, String & Int] = x
val z: String = y
