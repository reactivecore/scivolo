// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

/** The symbols and positions of `quotes.reflect`. */
trait ReflectSymbolSupport { this: QuotesImpl =>

trait ReflectSymbols extends reflectModule { self: reflect.type =>

  object Symbol extends SymbolModule {
    def spliceOwner: Symbol = symbol(ReflectOps.one("spliceOwner"))
    def noSymbol: Symbol = symbol(-1)
    def requiredPackage(path: String): Symbol = {
      ReflectOps.run("requiredPackage", new Array[Int](0), path)
      symbol(QuoteHost.result(0))
    }
    def requiredClass(path: String): Symbol = {
      ReflectOps.run("requiredClass", new Array[Int](0), path)
      symbol(QuoteHost.result(0))
    }
    def requiredModule(path: String): Symbol = refused("Symbol.requiredModule")
    def requiredMethod(path: String): Symbol = {
      ReflectOps.run("requiredMethod", new Array[Int](0), path)
      symbol(QuoteHost.result(0))
    }
    def classSymbol(fullName: String): Symbol = requiredClass(fullName)
    def newClass(owner: Symbol, name: String, parents: List[TypeRepr], decls: Symbol => List[Symbol],
        selfType: Option[TypeRepr]): Symbol = refused("Symbol.newClass")
    def newClass(owner: Symbol, name: String, parents: Symbol => List[TypeRepr], decls: Symbol => List[Symbol],
        selfType: Option[TypeRepr], clsFlags: Flags, clsPrivateWithin: Symbol,
        conParams: List[(String, TypeRepr)]): Symbol = refused("Symbol.newClass")
    def newClass(owner: Symbol, name: String, parents: Symbol => List[TypeRepr], decls: Symbol => List[Symbol],
        selfType: Option[TypeRepr], clsFlags: Flags, clsPrivateWithin: Symbol, clsAnnotations: List[Term],
        conMethodType: TypeRepr => MethodOrPoly, conFlags: Flags, conPrivateWithin: Symbol,
        conParamFlags: List[List[Flags]], conParamPrivateWithins: List[List[Symbol]]): Symbol =
      refused("Symbol.newClass")
    def newModule(owner: Symbol, name: String, modFlags: Flags, clsFlags: Flags, parents: Symbol => List[TypeRepr],
        decls: Symbol => List[Symbol], privateWithin: Symbol): Symbol = refused("Symbol.newModule")
    def newMethod(owner: Symbol, name: String, tpe: TypeRepr): Symbol =
      newMethod(owner, name, tpe, Flags.EmptyFlags, noSymbol)
    def newMethod(owner: Symbol, name: String, tpe: TypeRepr, flags: Flags, privateWithin: Symbol): Symbol =
      newTerm("newMethod", owner, name, tpe, flags, privateWithin)
    def newVal(owner: Symbol, name: String, tpe: TypeRepr, flags: Flags, privateWithin: Symbol): Symbol =
      newTerm("newVal", owner, name, tpe, flags, privateWithin)

    private def newTerm(op: String, owner: Symbol, name: String, tpe: TypeRepr, flags: Flags, privateWithin: Symbol) = {
      if (ReflectOps.id(privateWithin) >= 0) refused("Symbol.newVal or Symbol.newMethod with privateWithin")
      val bits = flags.asInstanceOf[FlagsImpl].bits
      ReflectOps.run(op, Array(ReflectOps.id(owner), ReflectOps.id(tpe), bits.toInt, (bits >>> 32).toInt), name)
      symbol(QuoteHost.result(0))
    }
    def newBind(owner: Symbol, name: String, flags: Flags, tpe: TypeRepr): Symbol = refused("Symbol.newBind")
    def newTypeAlias(owner: Symbol, name: String, flags: Flags, tpe: TypeRepr, privateWithin: Symbol): Symbol =
      refused("Symbol.newTypeAlias")
    def newBoundedType(owner: Symbol, name: String, flags: Flags, tpe: TypeBounds, privateWithin: Symbol): Symbol =
      refused("Symbol.newBoundedType")
    def freshName(prefix: String): String = {
      ReflectOps.run("freshName", new Array[Int](0), prefix)
      QuoteHost.resultText()
    }
  }

  object Position extends PositionModule {
    def ofMacroExpansion: Position = ReflectOps.position("expansionPos").asInstanceOf[Position]
    def apply(sourceFile: SourceFile, start: Int, end: Int): Position =
      new PositionImpl(sourceFile.asInstanceOf[SourceFileImpl].file, start, end).asInstanceOf[Position]
  }

  given SymbolMethods: SymbolMethods = methods("SymbolMethodsImpl")

  /** Kinds are numbered 0 to 6: package, class, type, type parameter, parameter, value, method;
   *  a package is its package class.
   */
  abstract class SymbolMethodsImpl extends SymbolMethods {
    extension (self: Symbol) {
      def companionModule: Symbol = sym(ReflectOps.one("companionModule", ReflectOps.id(self)))
      def declaredMethod(name: String): List[Symbol] = {
        val n = ReflectOps.run("declaredMethod", Array(ReflectOps.id(self)), name)
        List.tabulate(n)(i => sym(QuoteHost.result(i)))
      }
      def owner: Symbol = sym(ReflectOps.one("owner", ReflectOps.id(self)))
      def name: String = if (ReflectOps.id(self) < 0) "<none>" else ReflectOps.text("name", ReflectOps.id(self))
      def fullName: String = ReflectOps.text("fullName", ReflectOps.id(self))
      def flags: Flags = ReflectOps.flags(ReflectOps.id(self)).asInstanceOf[Flags]
      def moduleClass: Symbol = sym(ReflectOps.one("moduleClass", ReflectOps.id(self)))
      def companionClass: Symbol = sym(ReflectOps.one("companionClass", ReflectOps.id(self)))
      def typeRef: TypeRef =
        if (!self.isType) throw new Unsupported("Symbol.typeRef of a term")
        else QuoteHost.tpe(ReflectOps.one("symbolRef", ReflectOps.id(self))).asInstanceOf[TypeRef]
      def termRef: TermRef =
        if (!self.isTerm && !self.isPackageDef) throw new Unsupported("Symbol.termRef of a type")
        else QuoteHost.tpe(ReflectOps.one("symbolRef", ReflectOps.id(self))).asInstanceOf[TermRef]
      def primaryConstructor: Symbol = sym(ReflectOps.one("primaryConstructor", ReflectOps.id(self)))
      def caseFields: List[Symbol] = ReflectOps.results("caseFields", ReflectOps.id(self)).map(sym)
      def children: List[Symbol] = ReflectOps.results("children", ReflectOps.id(self)).map(sym)
      def paramSymss: List[List[Symbol]] = {
        val r = ReflectOps.results("paramSymss", ReflectOps.id(self))
        val sizes = r.slice(1, 1 + r.head)
        val params = r.drop(1 + r.head).map(sym)
        sizes.scanLeft(0)(_ + _).zip(sizes).map((start, n) => params.slice(start, start + n))
      }
      def methodMember(name: String): List[Symbol] = members(self, name)
      def memberMethod(name: String): List[Symbol] = members(self, name)
      def methodMembers: List[Symbol] = members(self, "")
      def memberMethods: List[Symbol] = members(self, "")
      def pos: Option[Position] =
        if (ReflectOps.id(self) < 0) None
        else Some(ReflectOps.position("symbolPos", ReflectOps.id(self)).asInstanceOf[Position])
      def annotations: List[Term] = annotated(self, -1)
      def getAnnotation(annotSym: Symbol): Option[Term] = annotated(self, ReflectOps.id(annotSym)).headOption
      def hasAnnotation(annotSym: Symbol): Boolean = annotated(self, ReflectOps.id(annotSym)).nonEmpty
      def tree: Tree = ReflectOps.opt(ReflectOps.one("symbolTree", ReflectOps.id(self))).getOrElse {
        throw new Unsupported("Symbol.tree of a symbol without a definition in the unit")
      }.asInstanceOf[Tree]
      def asQuotes: scala.quoted.Quotes = new QuotesImpl
      def info: TypeRepr = QuoteHost.tpe(ReflectOps.one("info", ReflectOps.id(self))).asInstanceOf[TypeRepr]
      def isNoSymbol: Boolean = ReflectOps.id(self) < 0
      def exists: Boolean = ReflectOps.id(self) >= 0
      def isPackageDef: Boolean = kind(self) == 0
      def isClassDef: Boolean = kind(self) == 0 || kind(self) == 1
      def isTypeDef: Boolean = (kind(self) == 2 || kind(self) == 3) && !isCase(self)
      def isValDef: Boolean = (kind(self) == 4 || kind(self) == 5) && !isCase(self)
      def isDefDef: Boolean = kind(self) == 6
      def isType: Boolean = kind(self) >= 0 && kind(self) <= 3
      def isTerm: Boolean = kind(self) >= 4
      def isClassConstructor: Boolean = self.exists && self.name == "<init>"
      def isLocalDummy: Boolean = self.isTerm && self.name.startsWith("<local ") && self.name.endsWith(">")
    }

    private def sym(id: Int): Symbol = new SymbolImpl(id).asInstanceOf[Symbol]

    private def members(s: Symbol, name: String): List[Symbol] = {
      val n = ReflectOps.run("methodMembers", Array(ReflectOps.id(s)), name)
      List.tabulate(n)(i => sym(QuoteHost.result(i)))
    }

    /** The annotations of `s`, only those of class `cls` unless it is `-1`. */
    private def annotated(s: Symbol, cls: Int): List[Term] =
      if (ReflectOps.id(s) < 0) Nil
      else ReflectOps.results("annotations", ReflectOps.id(s), cls).map(ReflectOps.term).asInstanceOf[List[Term]]

    private def kind(s: Symbol): Int = if (ReflectOps.id(s) < 0) -1 else ReflectOps.one("kind", ReflectOps.id(s))

    private def isCase(s: Symbol): Boolean = (ReflectOps.flags(ReflectOps.id(s)).bits & 1L << 12) != 0
  }

  given PositionMethods: PositionMethods = methods("PositionMethodsImpl")
  given SourceFileMethods: SourceFileMethods = methods("SourceFileMethodsImpl")

  abstract class PositionMethodsImpl extends PositionMethods {
    extension (self: Position) {
      def start: Int = self.asInstanceOf[PositionImpl].start
      def end: Int = self.asInstanceOf[PositionImpl].end
      def sourceFile: SourceFile = new SourceFileImpl(self.asInstanceOf[PositionImpl].file).asInstanceOf[SourceFile]
      def startLine: Int = self.asInstanceOf[PositionImpl].at(0)
      def startColumn: Int = self.asInstanceOf[PositionImpl].at(1)
      def endLine: Int = self.asInstanceOf[PositionImpl].at(2)
      def endColumn: Int = self.asInstanceOf[PositionImpl].at(3)
      def sourceCode: Option[String] = Option(self.asInstanceOf[PositionImpl].code)
    }
  }

  abstract class SourceFileMethodsImpl extends SourceFileMethods {
    extension (self: SourceFile) {
      def path: String = ReflectOps.text("sourcePath", self.asInstanceOf[SourceFileImpl].file)
      def name: String = self.path.substring(self.path.lastIndexOf('/') + 1)
      def content: Option[String] = Some(ReflectOps.text("sourceContent", self.asInstanceOf[SourceFileImpl].file))
    }
  }

  private def symbol(id: Int): Symbol = new SymbolImpl(id).asInstanceOf[Symbol]

  private def refused(what: String): Nothing = throw new Unsupported(what)

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectSymbolSupport$ReflectSymbols$" + name).asInstanceOf[T]
}
}
