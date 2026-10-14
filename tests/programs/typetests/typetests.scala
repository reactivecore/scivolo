//> using scala 3.9.0

def isTuple(x: Any) = x.isInstanceOf[Tuple]
def isPair(x: Any) = x.isInstanceOf[? *: ?]
def isEmpty(x: Any) = x.isInstanceOf[EmptyTuple]
def isNonEmpty(x: Any) = x.isInstanceOf[NonEmptyTuple]

def nullMatch(x: Null) = x match
  case Some(_) => "some"
  case (_, _) => "pair"
  case null => "null"

def nothingMatch(f: () => Nothing) =
  try
    f() match
      case Some(_) => "some"
      case _ => "any"
  catch case e: IllegalStateException => e.getMessage

@main def typetests =
  println(isTuple((1, 2)))
  println(isTuple("a"))
  println(isPair((1, "b", 3.0)))
  println(isPair(Some(1)))
  println(isEmpty(EmptyTuple))
  println(isNonEmpty(EmptyTuple))
  val n = null
  println(n.isInstanceOf[String])
  println(nullMatch(null))
  println(nothingMatch(() => throw IllegalStateException("thrown")))
  try
    val Some(_) = n
  catch case _: MatchError => println("match error")
