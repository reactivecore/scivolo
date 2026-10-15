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
        }
        let mut open = false;
        for inst in &body.insts {
            if let Inst::Label(l, params) = inst {
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
                if let Some(v) = self.op(id.0, op)? {
                    self.vals.insert(id.0, v);
                }
                if super::unwind::may_throw(op) {
                    self.after_call(unwind)?;
                }
                return Ok(false);
            }
            Inst::Throw(v, unwind) => {
                let exc = self.val(*v)?;
                self.throw(exc, unwind)?;
            }
            Inst::Ret(v) => {
                let ret = self.b.func.signature.returns.first().map(|r| r.value_type);
                let args = match ret {
                    Some(t) => vec![self.arg(*v, t)?],
                    None => vec![],
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
            // Temporary until #113 aot 5 (linktime conditions).
            Inst::Label(..) | Inst::LinktimeIf(..) => {
                return Err(format!("instruction not supported yet: {inst:?}"));
            }
        }
        Ok(true)
    }

    fn target(&mut self, next: &Next) -> R<(Block, Vec<BlockArg>)> {
        let Next::Label(l, args) = next else { return Err(format!("jump to {next:?}")) };
        let block = *self.blocks.get(&l.0).ok_or("unknown label")?;
        let types: Vec<types::Type> = self.b.func.dfg.block_param_types(block).collect();
        let mut out = Vec::with_capacity(args.len());
        for (&a, t) in args.iter().zip(types) {
            out.push(BlockArg::from(self.arg(a, t)?));
        }
        Ok((block, out))
    }

    pub fn jump(&mut self, next: &Next) -> R<()> {
        let (block, args) = self.target(next)?;
        self.b.ins().jump(block, &args);
        Ok(())
    }
}
