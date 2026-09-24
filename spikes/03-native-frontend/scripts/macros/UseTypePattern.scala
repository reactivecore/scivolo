import Macros.*

@main def useTypePattern(): Unit =
  println(s"kind: ${kind[Int]} / ${kind[String]} / ${kind[Boolean]}")
  println(s"listElem: ${listElem[List[Int]]} / ${listElem[Int]}")
