package scala.probe

import scala.beans.{BeanProperty, BooleanBeanProperty}
import scala.annotation.meta.beanGetter

class Beans(@BeanProperty val id: Int) {
  @BeanProperty val name: String = "n"
  @BeanProperty var count: Int = 0
  @BooleanBeanProperty var on: Boolean = false
  @(deprecated @beanGetter)("old", "1") @BeanProperty val old: Long = 1L
}
