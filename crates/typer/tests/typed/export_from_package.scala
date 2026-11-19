package foo {
  object Wrap {
    export foo.Bar
  }
  class Bar
  val wrapBar = new Wrap.Bar()
}
package a {
  opaque type T = Int
  def t: T = 0
}
package b {
  export a.T
  val t: b.T = a.t
}
