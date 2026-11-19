import scala.reflect.Selectable.reflectiveSelectable

object StructuralParamRefinedAlias {
  type Meat = { type IsMeat = Any }
  type Lion = {
    type Food = Meat
    def eats(food: Food): Unit
  }
  def leo: Lion = ???
  def m: Meat = ???
  def b = leo.eats(m)
}
