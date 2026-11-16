package p {
  package a {
    class A {
      protected def id[T](x: T): T = x
      protected def twice[T](x: T)(n: Int): String = x.toString * n
    }

    abstract class Outer {
      class B {
        trait Node { def s: String = "node" }
        protected def tie(x: Node): String = x.s
      }
    }
  }

  package b {
    class C extends p.a.A {
      class Inner {
        def run(): Unit = {
          println(id(1))
          println(id("a"))
          println(twice("b")(3))
        }
      }
    }

    abstract class X extends p.a.Outer {
      trait Inner extends B {
        def node: Node
        def other: Inner
        trait InnerInner {
          val g = other
          def run(): String = g.tie(node.asInstanceOf[g.Node])
        }
      }
    }
  }
}

@main def protectedpaths(): Unit = {
  val c = new p.b.C
  new c.Inner().run()
  val x = new p.b.X {}
  val i = new x.Inner {
    def node = new Node {}
    def other = this
  }
  println(new i.InnerInner {}.run())
}
