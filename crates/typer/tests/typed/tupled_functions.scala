//> using options -experimental

import scala.util.TupledFunction

object TupledFunctions:
  def tupled[F, Args <: Tuple, R](f: F)(using tf: TupledFunction[F, Args => R]): Args => R = tf.tupled(f)
  def untupled[F, Args <: Tuple, R](g: Args => R)(using tf: TupledFunction[F, Args => R]): F = tf.untupled(g)

  val add = (a: Int, b: Int) => a + b
  val t = tupled(add)
  val r = t((1, 2))
  val u: (Int, Int, Int) => Int = untupled((p: (Int, Int, Int)) => p._1 + p._2 + p._3)
  val c = summon[TupledFunction[(Int, String) ?=> Int, ((Int, String)) ?=> Int]]
  val none = summon[TupledFunction[() => Int, EmptyTuple => Int]]
