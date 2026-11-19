trait NewType:
  opaque type Type = Int
  def apply(value: Int): Type = value
  extension (self: Type)
    def value: Int = self
    def +(y: Type): Type = apply(self.value + y.value)

object Amount extends NewType:
  val x = Amount(1)
  val y = x + x
