object Q:
  final val X = 32

object Periods:
  inline val PhaseWidth = 7
  inline val RunWidth = java.lang.Integer.SIZE - PhaseWidth * 2 - 1
  inline val Below = Q.X - 1

@main def foldedinlinevals = println((Periods.RunWidth, Periods.Below))
