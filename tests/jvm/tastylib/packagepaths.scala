package scala.probe

import scala.collection.mutable._
import scala.probe.Clock.Time

object Clock:
  class Time

object Paths:
  def imported(t: Time): Time = t
  def selected(t: Clock.Time): ListBuffer[Int] = null
