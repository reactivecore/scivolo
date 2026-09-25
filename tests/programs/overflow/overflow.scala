//> using scala 3.9.0

import scala.scalanative.unsafe.*

def deep(n: Int): Int =
  val buf = stackalloc[Byte](64 * 1024)
  buf(0) = n.toByte
  deep(n + 1) + buf(0)

/** Recursion whose frames are large must end in a catchable StackOverflowError. The Scala Native
  * binary crashes here, so `overflow.check` is written by hand.
  */
@main def overflow(): Unit =
  val caught =
    try
      deep(0)
      false
    catch case _: StackOverflowError => true
  println(s"caught: $caught")
