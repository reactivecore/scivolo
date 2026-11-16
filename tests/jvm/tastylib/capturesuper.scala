package scala.probe

import language.experimental.captureChecking

trait Views {
  def view: Iterator[Int]^{this} = Iterator.empty
}

trait SeqViews extends Views {
  override def view: Iterator[Int]^{this} = super.view
}
