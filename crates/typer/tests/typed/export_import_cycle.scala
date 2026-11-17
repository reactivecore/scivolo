object Dec:
  import Ops.*
  type Names[NT <: AnyNT] = NT

object Ops:
  opaque type AnyNT = Any
  export Dec.*
  def x: Names[AnyNT] = 1.asInstanceOf[AnyNT]
