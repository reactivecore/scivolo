//! Blocks, values and control flow of one function body.

use super::{R, Symbols, value_type};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::immediates::{Ieee32, Ieee64};
use cranelift_codegen::ir::{Block, BlockArg, InstBuilder, TrapCode, Value, types};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Module};
use cranelift_object::ObjectModule;
use nir::{Body, GlobalId, Inst, Next, Type, Val, ValId};
use std::collections::HashMap;

pub struct FnBuilder<'a, 'p> {
    pub b: FunctionBuilder<'a>,
    pub module: &'a mut ObjectModule,
    pub sym: &'a mut Symbols<'p>,
    blocks: HashMap<u64, Block>,
    vals: HashMap<u64, Value>,
    /// Locals holding a module instance, by module.
    pub module_vals: HashMap<u64, GlobalId>,
    /// Locals holding a statically resolved method.
    pub methods: HashMap<u64, FuncId>,
}

impl<'a, 'p> FnBuilder<'a, 'p> {
    pub fn new(b: FunctionBuilder<'a>, module: &'a mut ObjectModule, sym: &'a mut Symbols<'p>) -> Self {
        Self {
            b,
            module,
            sym,
            blocks: HashMap::new(),
            vals: HashMap::new(),
            module_vals: HashMap::new(),
            methods: HashMap::new(),
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
                    self.b.append_block_param(block, value_type(self.sym.pool, p.ty)?);
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
        self.b.seal_all_blocks();
        self.b.finalize(self.module.target_config());
    }

    /// Translates one instruction; `true` when it ends the block.
    fn inst(&mut self, inst: &Inst) -> R<bool> {
        match inst {
            Inst::Let { id, op, unwind, .. } => {
                if !matches!(unwind, Next::None) {
                    // Temporary until #113 aot 4 (exceptions as flags).
                    return Err("exception handlers are not supported yet".into());
                }
                if let Some(v) = self.op(id.0, op)? {
                    self.vals.insert(id.0, v);
                }
                return Ok(false);
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
            // Temporary until #113 aot 4 (throw) and aot 5 (linktime conditions).
            Inst::Label(..) | Inst::Throw(..) | Inst::LinktimeIf(..) => {
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

    fn jump(&mut self, next: &Next) -> R<()> {
        let (block, args) = self.target(next)?;
        self.b.ins().jump(block, &args);
        Ok(())
    }

    pub fn val(&mut self, v: ValId) -> R<Value> {
        let pool = self.sym.pool;
        let int = |b: &mut FunctionBuilder, t, x: i64| b.ins().iconst(t, x);
        Ok(match pool.val(v) {
            Val::Local(l, ty) => match self.vals.get(&l.0) {
                Some(&v) => v,
                None if matches!(pool.ty(*ty), Type::Unit) => int(&mut self.b, types::I8, 0),
                None => return Err(format!("undefined local %{}", l.0)),
            },
            Val::True => int(&mut self.b, types::I8, 1),
            Val::False | Val::Unit => int(&mut self.b, types::I8, 0),
            Val::Null => int(&mut self.b, types::I64, 0),
            Val::Zero(ty) => match value_type(pool, *ty)? {
                types::F32 => self.b.ins().f32const(0.0),
                types::F64 => self.b.ins().f64const(0.0),
                t => int(&mut self.b, t, 0),
            },
            Val::Char(c) => int(&mut self.b, types::I16, *c as i64),
            Val::Byte(x) => int(&mut self.b, types::I8, *x as u8 as i64),
            Val::Short(x) => int(&mut self.b, types::I16, *x as u16 as i64),
            Val::Int(x) => int(&mut self.b, types::I32, *x as u32 as i64),
            Val::Long(x) => int(&mut self.b, types::I64, *x),
            Val::Size(x) => int(&mut self.b, types::I64, *x as i64),
            Val::Float(bits) => self.b.ins().f32const(Ieee32::with_bits(*bits)),
            Val::Double(bits) => self.b.ins().f64const(Ieee64::with_bits(*bits)),
            v => return Err(format!("value not supported yet: {v:?}")),
        })
    }

    /// `v` where a value of type `t` is expected: `unit` and `null` become zero of type `t`.
    pub fn arg(&mut self, v: ValId, t: types::Type) -> R<Value> {
        let pool = self.sym.pool;
        match pool.val(v) {
            Val::Unit | Val::Null => Ok(self.b.ins().iconst(t, 0)),
            Val::Local(_, ty) if matches!(pool.ty(*ty), Type::Unit) => Ok(self.b.ins().iconst(t, 0)),
            _ => self.val(v),
        }
    }
}
