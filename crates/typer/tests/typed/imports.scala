import scala.collection.mutable
import scala.math.{Pi as P, E}

object Constants:
  val zero = 0
  val pi: Double = P
  val e: Double = E

object Use:
  import Constants.*
  def both: Double = pi
  val buf: mutable.ArrayBuffer[Int] = null
