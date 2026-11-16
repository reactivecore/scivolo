package scala.probe

import language.experimental.captureChecking

class Outer extends caps.SharedCapability {
  trait Views {
    def view: Iterator[Int]^{Outer.this} = Iterator.empty
  }
  val other: Outer = this
  trait SeqViews extends other.Views {
    def size: Int = super.view.size
  }
}
