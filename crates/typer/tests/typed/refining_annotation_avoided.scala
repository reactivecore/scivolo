import scala.annotation.RefiningAnnotation

class tagged extends RefiningAnnotation

type Tagged = Int @tagged

def test =
  val result =
    val tmp: Tagged = ???
    tmp
  result
