import scala.annotation.unchecked.uncheckedVariance
import scala.compiletime.uninitialized

class Type

abstract class Tree[+T <: Type | Null]:
  private var myTpe: T @uncheckedVariance = uninitialized
  final def tpe: T =
    if myTpe == null then throw IllegalStateException("unassigned")
    myTpe

@main def annotatedequality =
  try println(new Tree[Type] {}.tpe)
  catch case e: IllegalStateException => println(e.getMessage)
