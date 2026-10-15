//! Blocks, values and control flow of one function body.

use super::Decls;
use crate::R;
use crate::reach::Reach;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{Block, BlockArg, InstBuilder, TrapCode, Value, types};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Module};
use cranelift_object::ObjectModule;
use nir::{Body, Inst, Next, TypeId};
use std::collections::HashMap;

pub struct FnBuilder<'a, 'w> {
    pub b: FunctionBuilder<'a>,
    pub module: &'a mut ObjectModule,
    pub reach: &'a mut Reach<'w>,
    pub decls: &'a mut Decls,
    blocks: HashMap<u64, Block>,
    pub vals: HashMap<u64, Value>,
    /// Locals holding a statically resolved method.
    pub methods: HashMap<u64, FuncId>,
    /// The block returning to the caller with an exception pending.
    pub propagate: Option<Block>,
    /// The type of each `var`.
    pub var_types: HashMap<u64, nir::TypeId>,
    /// The unwind edge of the instruction being translated.
    pub unwind: Next,
    /// The shared block of each runtime throw without a handler.
    pub fail_blocks: HashMap<&'static str, Block>,
    /// Locals known not to be null: fresh objects.
    pub non_null: std::collections::HashSet<u64>,
    /// The NIR types of each block's parameters.
    param_types: HashMap<u64, Vec<nir::TypeId>>,
    /// The NIR result type of the function.
    pub ret_ty: Option<nir::TypeId>,
    /// Locals holding a `@resolvedAtLinktime` method, whose calls are its value.
    pub linktime_methods: HashMap<u64, nir::GlobalId>,
}

impl<'a, 'w> FnBuilder<'a, 'w> {
    pub fn new(
        b: FunctionBuilder<'a>,
        module: &'a mut ObjectModule,
        reach: &'a mut Reach<'w>,
        decls: &'a mut Decls,
    ) -> Self {
        Self {
            b,
            module,
            reach,
            decls,
            blocks: HashMap::new(),
            vals: HashMap::new(),
            methods: HashMap::new(),
            propagate: None,
            var_types: HashMap::new(),
            unwind: Next::None,
            fail_blocks: HashMap::new(),
            non_null: Default::default(),
            param_types: HashMap::new(),
            ret_ty: None,
            linktime_methods: HashMap::new(),
        }
    }

    pub fn body(&mut self, body: &Body) -> R<()> {
        for (n, inst) in body.insts.iter().enumerate() {
            let Inst::Label(l, params) = inst else { continue };
            let block = self.b.create_block();
            if n == 0 {
                self.b.append_block_params_for_function_params(block);
            } else {
                for p in params {
                    let t = self.ty(p.ty)?;
                    self.b.append_block_param(block, t);
                }
            }
            self.blocks.insert(l.0, block);
            self.param_types.insert(l.0, params.iter().map(|p| p.ty).collect());
        }
        let reachable = super::blocks::reachable(&self.reach.world.pool, body)?;
        let mut open = false;
        for inst in &body.insts {
            if let Inst::Label(l, params) = inst {
                if !reachable.contains(&l.0) {
                    open = false;
                    continue;
                }
                let block = self.blocks[&l.0];
                self.b.switch_to_block(block);
                let values = self.b.block_params(block).to_vec();
                self.vals.extend(params.iter().map(|p| p.id.0).zip(values));
                open = true;
                continue;
            }
            if open {
                open = !self.inst(inst)?;
            }
        }
        match open {
            true => Err("a block without terminator".into()),
            false => Ok(()),
        }
    }

    pub fn finish(mut self) {
        self.define_propagate();
        self.b.seal_all_blocks();
        self.b.finalize(self.module.target_config());
    }

    /// The Cranelift type of NIR type `t`.
    pub fn ty(&self, t: TypeId) -> R<types::Type> {
        self.decls.value_type(&self.reach.world.pool, t)
    }

    /// Translates one instruction; `true` when it ends the block.
    fn inst(&mut self, inst: &Inst) -> R<bool> {
        match inst {
            Inst::Let { id, op, unwind, .. } => {
                self.unwind = unwind.clone();
                if let Some(v) = self.op(id.0, op)? {
                    self.vals.insert(id.0, v);
                }
                if super::unwind::may_throw(op) {
                    self.after_call(unwind)?;
                }
                return Ok(false);
            }
            Inst::Throw(v, unwind) => self.throw(*v, unwind)?,
            Inst::Ret(v) => {
                let ret = self.b.func.signature.returns.first().map(|r| r.value_type);
                let args = match (ret, self.ret_ty) {
                    (Some(_), Some(ty)) => vec![self.arg_of(*v, ty)?],
                    (Some(t), None) => vec![self.arg(*v, t)?],
                    (None, _) => vec![],
                };
                self.b.ins().return_(&args);
            }
            Inst::Jump(next) => self.jump(next)?,
            Inst::If(c, then, els) => {
                let c = self.val(*c)?;
                let (then, then_args) = self.target(then)?;
                let (els, els_args) = self.target(els)?;
                if then == els {
                    let via = self.b.create_block();
                    self.b.ins().brif(c, then, &then_args, via, &[]);
                    self.b.switch_to_block(via);
                    self.b.ins().jump(els, &els_args);
                } else {
                    self.b.ins().brif(c, then, &then_args, els, &els_args);
                }
            }
            Inst::Switch(scrut, default, cases) => {
                let s = self.val(*scrut)?;
                for case in cases {
                    let Next::Case(v, to) = case else { return Err("switch case".into()) };
                    let v = self.val(*v)?;
                    let eq = self.b.ins().icmp(IntCC::Equal, s, v);
                    let (block, args) = self.target(to)?;
                    let next = self.b.create_block();
                    self.b.ins().brif(eq, block, &args, next, &[]);
                    self.b.switch_to_block(next);
                }
                self.jump(default)?;
            }
            Inst::Unreachable(_) => {
                self.b.ins().trap(TrapCode::unwrap_user(1));
            }
            Inst::LinktimeIf(cond, then, els) => {
                let taken = super::linktime::decide(&self.reach.world.pool, cond)?;
                self.jump(if taken { then } else { els })?;
            }
            Inst::Label(..) => {
                return Err(format!("instruction not supported yet: {inst:?}"));
            }
        }
        Ok(true)
    }

    fn target(&mut self, next: &Next) -> R<(Block, Vec<BlockArg>)> {
        let Next::Label(l, args) = next else { return Err(format!("jump to {next:?}")) };
        let block = *self.blocks.get(&l.0).ok_or("unknown label")?;
        let types = self.param_types.get(&l.0).cloned().unwrap_or_default();
        let mut out = Vec::with_capacity(args.len());
        for (&a, ty) in args.iter().zip(types) {
            out.push(BlockArg::from(self.arg_of(a, ty)?));
        }
        Ok((block, out))
    }

    pub fn jump(&mut self, next: &Next) -> R<()> {
        let (block, args) = self.target(next)?;
        self.b.ins().jump(block, &args);
        Ok(())
    }
}
