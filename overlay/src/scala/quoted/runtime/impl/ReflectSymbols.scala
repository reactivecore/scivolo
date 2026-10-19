package scala.quoted.runtime.impl

/** The symbols and positions of `quotes.reflect`. */
trait ReflectSymbolSupport { this: QuotesImpl =>

trait ReflectSymbols extends reflectModule { self: reflect.type =>

  object Symbol extends SymbolModule {
    def spliceOwner: Symbol = symbol(ReflectOps.one("spliceOwner"))
    def noSymbol: Symbol = symbol(-1)
    def requiredPackage(path: String): Symbol = refused("Symbol.requiredPackage")
    def requiredClass(path: String): Symbol = refused("Symbol.requiredClass")
    def requiredModule(path: String): Symbol = refused("Symbol.requiredModule")
    def requiredMethod(path: String): Symbol = refused("Symbol.requiredMethod")
    def classSymbol(fullName: String): Symbol = refused("Symbol.classSymbol")
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
    def newMethod(owner: Symbol, name: String, tpe: TypeRepr): Symbol = refused("Symbol.newMethod")
    def newMethod(owner: Symbol, name: String, tpe: TypeRepr, flags: Flags, privateWithin: Symbol): Symbol =
      refused("Symbol.newMethod")
    def newVal(owner: Symbol, name: String, tpe: TypeRepr, flags: Flags, privateWithin: Symbol): Symbol =
      refused("Symbol.newVal")
    def newBind(owner: Symbol, name: String, flags: Flags, tpe: TypeRepr): Symbol = refused("Symbol.newBind")
    def newTypeAlias(owner: Symbol, name: String, flags: Flags, tpe: TypeRepr, privateWithin: Symbol): Symbol =
      refused("Symbol.newTypeAlias")
    def newBoundedType(owner: Symbol, name: String, flags: Flags, tpe: TypeBounds, privateWithin: Symbol): Symbol =
      refused("Symbol.newBoundedType")
    def freshName(prefix: String): String = refused("Symbol.freshName")
  }

  object Position extends PositionModule {
    def ofMacroExpansion: Position = ReflectOps.position("expansionPos").asInstanceOf[Position]
    def apply(sourceFile: SourceFile, start: Int, end: Int): Position =
      new PositionImpl(sourceFile.asInstanceOf[SourceFileImpl].file, start, end).asInstanceOf[Position]
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
