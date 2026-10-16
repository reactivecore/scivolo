//> using scala 3.9.0

import scala.scalanative.unsafe.*

def deep(n: Int): Int =
  val buf = stackalloc[Byte](64 * 1024)
  buf(0) = n.toByte
  deep(n + 1) + buf(0)

/** Recursion whose frames are large must end in a catchable StackOverflowError, again after the
  * first one. The Scala Native binary crashes here, so `overflow.check` is written by hand.
  */
@main def overflow(): Unit =
  for _ <- 1 to 2 do
    try
      deep(0)
      println("no overflow")
    catch
      case e: StackOverflowError =>
        val top = e.getStackTrace.head
        println(s"caught in ${top.getMethodName} ${top.getFileName}:${top.getLineNumber}")
