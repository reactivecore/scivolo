object Test {
  def main(args: Array[String]): Unit = {
    {
      object outer {
        object Inner {
          trait Leaf
        }
      }
      val a: outer.Inner.type = outer.Inner
    }
    {
      object outer {
        object Inner {
          object Leaf
        }
      }
      object alias {
        val Inner: outer.Inner.type = outer.Inner
      }
      val b: alias.Inner.Leaf.type = alias.Inner.Leaf
    }
  }
}
