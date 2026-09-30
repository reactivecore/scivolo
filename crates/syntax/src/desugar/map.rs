//! A tree with its children transformed, as dotty's `UntypedTreeMap` copies it.

use super::Desugar;
use crate::ast::{NodeId, Tree};

impl Desugar<'_> {
    pub fn map_children(&mut self, id: NodeId) -> NodeId {
        use Tree::*;
        let tree = match self.ast.tree(id).clone() {
            Ident { .. }
            | This { .. }
            | Literal { .. }
            | TypeTree
            | SymbolLit { .. }
            | Number { .. }
            | TypedSplice { .. }
            | DerivedTypeTree { .. }
            | Empty => return id,
            Template { .. } => return self.transform_template(id),
            Block { .. } => return self.transform_block(id),
            Thicket { trees } => {
                let mapped = self.transform_list(trees);
                return if mapped == trees { id } else { self.ast.node(Thicket { trees: mapped }) };
            }
            PackageDef { pid, stats } => {
                PackageDef { pid: self.transform(pid), stats: self.transform_stats(stats) }
            }
            Select { qualifier, name } => Select { qualifier: self.transform(qualifier), name },
            Super { qual, mix } => Super { qual: self.transform(qual), mix },
            Apply { fun, args } => Apply { fun: self.transform(fun), args: self.transform_list(args) },
            TypeApply { fun, args } => {
                TypeApply { fun: self.transform(fun), args: self.transform_list(args) }
            }
            New { tpt } => New { tpt: self.transform(tpt) },
            Typed { expr, tpt } => Typed { expr: self.transform(expr), tpt: self.transform(tpt) },
            NamedArg { name, arg } => NamedArg { name, arg: self.transform(arg) },
            Assign { lhs, rhs } => Assign { lhs: self.transform(lhs), rhs: self.transform(rhs) },
            If { cond, thenp, elsep, inline } => If {
                cond: self.transform(cond),
                thenp: self.transform(thenp),
                elsep: self.transform_opt(elsep),
                inline,
            },
            Match { selector, cases, kind } => {
                Match { selector: self.transform_opt(selector), cases: self.transform_list(cases), kind }
            }
            CaseDef { pat, guard, body } => CaseDef {
                pat: self.transform(pat),
                guard: self.transform_opt(guard),
                body: self.transform(body),
            },
            Return { expr } => Return { expr: self.transform_opt(expr) },
            WhileDo { cond, body } => WhileDo { cond: self.transform(cond), body: self.transform(body) },
            Quote { body } => Quote { body: self.transform(body) },
            Splice { expr } => Splice { expr: self.transform(expr) },
            SplicePattern { body } => SplicePattern { body: self.transform(body) },
            SingletonTypeTree { reference } => SingletonTypeTree { reference: self.transform(reference) },
            RefinedTypeTree { tpt, refinements } => RefinedTypeTree {
                tpt: self.transform_opt(tpt),
                refinements: self.transform_list(refinements),
            },
            AppliedTypeTree { tpt, args } => {
                AppliedTypeTree { tpt: self.transform(tpt), args: self.transform_list(args) }
            }
            LambdaTypeTree { tparams, body } => {
                LambdaTypeTree { tparams: self.transform_list(tparams), body: self.transform(body) }
            }
            TermLambdaTypeTree { params, body } => {
                TermLambdaTypeTree { params: self.transform_list(params), body: self.transform(body) }
            }
            MatchTypeTree { bound, selector, cases } => MatchTypeTree {
                bound: self.transform_opt(bound),
                selector: self.transform(selector),
                cases: self.transform_list(cases),
            },
            ByNameTypeTree { result } => ByNameTypeTree { result: self.transform(result) },
            TypeBoundsTree { lo, hi, alias } => TypeBoundsTree {
                lo: self.transform_opt(lo),
                hi: self.transform_opt(hi),
                alias: self.transform_opt(alias),
            },
            Bind { name, body, mods } => Bind { name, body: self.transform(body), mods },
            Alternative { trees } => Alternative { trees: self.transform_list(trees) },
            ValDef { name, tpt, rhs, mods } => {
                ValDef { name, tpt: self.transform(tpt), rhs: self.transform_opt(rhs), mods }
            }
            DefDef { .. } | TypeDef { .. } => return self.transform(id),
            Import { expr, selectors } => Import { expr: self.transform(expr), selectors },
            Export { expr, selectors } => Export { expr: self.transform(expr), selectors },
            Annotated { arg, annot } => Annotated { arg: self.transform(arg), annot: self.transform(annot) },
            ModuleDef { name, imp, mods } => ModuleDef { name, imp: self.transform(imp), mods },
            ParsedTry { expr, handler, finalizer } => ParsedTry {
                expr: self.transform(expr),
                handler: self.transform_opt(handler),
                finalizer: self.transform_opt(finalizer),
            },
            Try { expr, cases, finalizer } => Try {
                expr: self.transform(expr),
                cases: self.transform_list(cases),
                finalizer: self.transform_opt(finalizer),
            },
            InterpolatedString { id: name, segments } => {
                let items = self.ast.items(segments).to_vec();
                let mapped: Vec<NodeId> = items.iter().map(|&t| self.transform(t)).collect();
                let segments = if mapped == items { segments } else { self.list(&mapped) };
                InterpolatedString { id: name, segments }
            }
            Function { args, body, mods, wildcard } => {
                Function { args: self.transform_list(args), body: self.transform(body), mods, wildcard }
            }
            PolyFunction { targs, body } => {
                PolyFunction { targs: self.transform_list(targs), body: self.transform(body) }
            }
            InfixOp { left, op, right } => {
                InfixOp { left: self.transform(left), op, right: self.transform(right) }
            }
            PostfixOp { od, op } => PostfixOp { od: self.transform(od), op },
            PrefixOp { op, od } => PrefixOp { op, od: self.transform(od) },
            Parens { t } => Parens { t: self.transform(t) },
            Tuple { trees } => Tuple { trees: self.transform_list(trees) },
            Throw { expr } => Throw { expr: self.transform(expr) },
            ForYield { enums, expr } => {
                ForYield { enums: self.transform_list(enums), expr: self.transform(expr) }
            }
            ForDo { enums, body } => ForDo { enums: self.transform_list(enums), body: self.transform(body) },
            GenFrom { pat, expr, check_mode } => {
                GenFrom { pat: self.transform(pat), expr: self.transform(expr), check_mode }
            }
            GenAlias { pat, expr } => GenAlias { pat: self.transform(pat), expr: self.transform(expr) },
            ContextBounds { bounds, cx_bounds } => {
                ContextBounds { bounds: self.transform(bounds), cx_bounds: self.transform_list(cx_bounds) }
            }
            PatDef { mods, pats, tpt, rhs } => PatDef {
                mods,
                pats: self.transform_list(pats),
                tpt: self.transform(tpt),
                rhs: self.transform_opt(rhs),
            },
            ExtMethods { paramss, methods } => {
                ExtMethods { paramss: self.transform_clauses(paramss), methods: self.transform_list(methods) }
            }
            ContextBoundTypeTree { tycon, param_name, own_name } => {
                ContextBoundTypeTree { tycon: self.transform(tycon), param_name, own_name }
            }
            MacroTree { expr } => MacroTree { expr: self.transform(expr) },
            ImportSelector { imported, renamed, bound } => ImportSelector {
                imported: self.transform(imported),
                renamed: self.transform_opt(renamed),
                bound: self.transform_opt(bound),
            },
            CapturesAndResult { refs, parent } => {
                CapturesAndResult { refs: self.transform_list(refs), parent: self.transform(parent) }
            }
            UseRef { reference, initially } => UseRef { reference: self.transform(reference), initially },
        };
        self.ast.derived(id, tree)
    }
}
