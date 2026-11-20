object PathCapturedArgs {
  trait Level
  trait FlatLevel extends Level
  abstract class Shape[L <: Level, -M, U, P]

  def pair[L <: Level, M1, M2, U1, U2, P1, P2](
      u1: Shape[? <: L, M1, U1, P1],
      u2: Shape[? <: L, M2, U2, P2]
  ): Shape[L, (M1, M2), (U1, U2), (P1, P2)] = ???

  val shape: Shape[? <: FlatLevel, Int, Int, ?] = null
  def hint = pair(shape, shape)
  val hint2: Shape[FlatLevel, (Int, Int), (Int, Int), ?] = hint
}
