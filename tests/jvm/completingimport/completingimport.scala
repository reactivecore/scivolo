package completingimport

type U = Long

object A {
  import completingimport.B.C.*
  type T = U
  def a: T = 1L
  def c: U = 2
}

object B {
  export A.*
  object C { type U = Int }
  def b: T = a
}

@main def Test = println(s"${B.b} ${A.c}")
