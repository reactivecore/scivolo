package scala.probe

import language.experimental.captureChecking

class Outer extends caps.SharedCapability {
  trait Views {
    def view: Iterator[Int]^{Outer.this} = Iterator.empty
  }

  trait SeqViews extends Views {
    override def view: Iterator[Int]^{Outer.this} = super.view
  }
}
