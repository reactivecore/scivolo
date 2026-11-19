package A {
  opaque type T = Int
  def t: T = 0
  class C
  package inner { class D }
}
package B {
  export A.{T, t, C}
  val t2: B.T = A.t
  val t3: T = t
  val c: C = new C
}
