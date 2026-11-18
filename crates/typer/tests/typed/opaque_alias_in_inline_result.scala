object Time:
  opaque type Time = String
  opaque type Seconds <: Time = String

  transparent inline def sec(n: Double): Seconds =
    s"${n}s": Seconds

  transparent inline def guarded(): List[Time] =
    List(sec(5)): List[Seconds]

  transparent inline def explicitTime(): List[Time] =
    List[Seconds](sec(5))

object Use:
  val a: List[Time.Seconds] = Time.guarded()
  val b: List[Time.Seconds] = Time.explicitTime()
