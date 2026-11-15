package scala.probe

import scala.annotation.switch

object Selectors {
  var ch: Char = 'a'

  def letter(): Int = (ch: @switch) match {
    case 'A' | 'B' => 1
    case _ => 0
  }

  def digit(): Int = (ch: @switch) match {
    case '0' => 1
    case _ => 0
  }

  def outerFirst(vparams: List[String] @unchecked): List[String] = vparams match {
    case Nil => Nil
    case _ => vparams.reverse
  }
}
