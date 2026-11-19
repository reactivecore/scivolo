object RefinementOfMemberWithTypeMember {
  trait C { type T; def process(x: T): Int }
  val y1: C { def process(x: T): Int } = ???
  val y2: C { type T; val key: T; def process(x: T): Int } = ???
  def z(x: y1.T): Int = y1.process(x)
}
