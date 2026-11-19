trait Test {
  abstract class Impl {
    trait Inner
  }
  val parens: Impl
  val bracks: Impl
  def inner(b: Boolean, p: parens.Inner, q: bracks.Inner) = if b then p else q
}
