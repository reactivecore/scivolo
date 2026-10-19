//> using dep io.circe::circe-parser::0.14.10
//> using dep io.circe::circe-generic::0.14.10

import io.circe.Codec, io.circe.syntax.*, io.circe.parser.decode

case class Person(id: Int, name: String) derives Codec.AsObject

@main def circe(): Unit =
  println(Person(1, "a").asJson.noSpaces)
  println(decode[Person]("""{"id":1,"name":"a"}"""))
