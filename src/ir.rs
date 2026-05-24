//! Vajra IR — Target-independent Intermediate Representation
//! SSA-like 3-address code used between AST and machine code generation.
//! Each IrValue is an immutable virtual register; mutable state is managed
//! through explicit Load/Store into named stack slots.

use std::collections::HashMap;

/// A virtual register ID (SSA value).
pub type ValId = u32;

/// A basic block ID within a function.
pub type BlockId = u32;

#[derive(Debug, Clone, PartialEq)]
pub enum IrType {
    I64,
    I32,
    I16,
    I8,
    F64,
    Bool,
    Ptr,
    Void,
}

impl IrType {
    pub fn size_bytes(&self) -> usize {
        match self {
            IrType::I64 | IrType::F64 | IrType::Ptr => 8,
            IrType::I32 => 4,
            IrType::I16 => 2,
            IrType::I8 | IrType::Bool => 1,
            IrType::Void => 0,
        }
    }
}

/// Comparison predicates
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        }
    }
}

/// An IR instruction producing a value (or Void).
#[derive(Debug, Clone)]
pub enum IrInstr {
    /// val = const_i64(n)
    ConstI64(ValId, i64),
    /// val = const_f64(f)
    ConstF64(ValId, f64),
    /// val = const_bool(b)
    ConstBool(ValId, bool),
    /// val = str_ptr("text") — points into read-only data
    StrPtr(ValId, String),
    /// val = alloca(ty) — allocate stack slot, returns pointer
    Alloca(ValId, IrType),
    /// store val → ptr
    Store(ValId, ValId),
    /// val = load ptr
    Load(ValId, ValId, IrType),
    /// val = add(a, b)
    Add(ValId, ValId, ValId),
    /// val = sub(a, b)
    Sub(ValId, ValId, ValId),
    /// val = mul(a, b)
    Mul(ValId, ValId, ValId),
    /// val = div(a, b)
    Div(ValId, ValId, ValId),
    /// val = rem(a, b)
    Rem(ValId, ValId, ValId),
    /// val = neg(a)
    Neg(ValId, ValId),
    /// val = fadd(a, b)
    FAdd(ValId, ValId, ValId),
    /// val = fsub(a, b)
    FSub(ValId, ValId, ValId),
    /// val = fmul(a, b)
    FMul(ValId, ValId, ValId),
    /// val = fdiv(a, b)
    FDiv(ValId, ValId, ValId),
    /// val = cmp(op, a, b)
    Cmp(ValId, CmpOp, ValId, ValId),
    /// val = and(a, b)
    And(ValId, ValId, ValId),
    /// val = or(a, b)
    Or(ValId, ValId, ValId),
    /// val = not(a)
    Not(ValId, ValId),
    /// val = call(func_name, args)
    Call(ValId, String, Vec<ValId>),
    /// val = call_indirect(func_ptr, args)
    CallIndirect(ValId, ValId, Vec<ValId>),
    /// val = syscall(num, args) — raw OS syscall
    SysCall(ValId, ValId, Vec<ValId>),
    /// val = gep(ptr, index) — get element pointer (array indexing)
    Gep(ValId, ValId, ValId),
    /// Atomic add: atomicadd(ptr, val) — for parallel loops
    AtomicAdd(ValId, ValId),
    /// Phi node: val = phi [(val1, block1), (val2, block2)]
    Phi(ValId, Vec<(ValId, BlockId)>),
    /// i64→f64 cast
    ItoF(ValId, ValId),
    /// f64→i64 cast
    FtoI(ValId, ValId),
    /// Zero-extend i1 → i64
    ZExt(ValId, ValId),
    /// Sign-extend smaller → i64
    SExt(ValId, ValId),
    /// Truncate i64 → i32
    Trunc(ValId, ValId, IrType),
    /// Bit cast (ptr ↔ int)
    BitCast(ValId, ValId, IrType),
    /// No-op / comment for debugging
    Comment(String),
}

/// Block terminator — exactly one per block
#[derive(Debug, Clone)]
pub enum IrTerminator {
    /// Return a value (Void returns use ConstI64(_, 0))
    Ret(ValId),
    /// Unconditional jump
    Jump(BlockId),
    /// Conditional branch: cond != 0 → then_block, else → else_block
    Branch(ValId, BlockId, BlockId),
    /// Unreachable (after throw / noreturn calls)
    Unreachable,
}

/// A basic block in an IR function
#[derive(Debug, Clone)]
pub struct IrBlock {
    pub id: BlockId,
    pub label: String,
    pub instrs: Vec<IrInstr>,
    pub terminator: Option<IrTerminator>,
}

impl IrBlock {
    pub fn new(id: BlockId, label: impl Into<String>) -> Self {
        Self { id, label: label.into(), instrs: Vec::new(), terminator: None }
    }
}

/// A parameter to an IR function
#[derive(Debug, Clone)]
pub struct IrParam {
    pub val: ValId,
    pub name: String,
    pub ty: IrType,
}

/// A complete IR function
#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<IrParam>,
    pub return_type: IrType,
    pub blocks: Vec<IrBlock>,
    pub is_extern: bool,
    pub is_main: bool,
}

impl IrFunction {
    pub fn new(name: impl Into<String>, return_type: IrType) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            return_type,
            blocks: Vec::new(),
            is_extern: false,
            is_main: false,
        }
    }

    pub fn entry_block(&self) -> Option<&IrBlock> {
        self.blocks.first()
    }

    pub fn entry_block_mut(&mut self) -> Option<&mut IrBlock> {
        self.blocks.first_mut()
    }
}

/// A global constant (string, integer, etc.)
#[derive(Debug, Clone)]
pub struct IrGlobal {
    pub name: String,
    pub data: Vec<u8>,
    pub is_string: bool,
}

/// A complete IR module (one per source file or compilation unit)
#[derive(Debug, Clone)]
pub struct IrModule {
    pub name: String,
    pub functions: Vec<IrFunction>,
    pub globals: Vec<IrGlobal>,
    pub extern_functions: Vec<String>,
}

impl IrModule {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            functions: Vec::new(),
            globals: Vec::new(),
            extern_functions: Vec::new(),
        }
    }

    pub fn merge(&mut self, other: IrModule) {
        for func in other.functions {
            if !self.functions.iter().any(|f| f.name == func.name) {
                self.functions.push(func);
            }
        }
        for global in other.globals {
            if !self.globals.iter().any(|g| g.name == global.name) {
                self.globals.push(global);
            }
        }
        for ext in other.extern_functions {
            if !self.extern_functions.contains(&ext) {
                self.extern_functions.push(ext);
            }
        }
    }
}

/// IR Builder — stateful builder for constructing IR functions
pub struct IrBuilder {
    next_val: ValId,
    next_block: BlockId,
    pub current_block: BlockId,
    pub function: IrFunction,
    /// Stack slot name → ValId of the alloca pointer
    pub named_slots: HashMap<String, ValId>,
}

impl IrBuilder {
    pub fn new(name: impl Into<String>, return_type: IrType) -> Self {
        let entry_block = IrBlock::new(0, "entry");
        Self {
            next_val: 0,
            next_block: 1,
            current_block: 0,
            function: IrFunction {
                name: name.into(),
                params: Vec::new(),
                return_type,
                blocks: vec![entry_block],
                is_extern: false,
                is_main: false,
            },
            named_slots: HashMap::new(),
        }
    }

    pub fn fresh_val(&mut self) -> ValId {
        let v = self.next_val;
        self.next_val += 1;
        v
    }

    pub fn fresh_block(&mut self, label: impl Into<String>) -> BlockId {
        let id = self.next_block;
        self.next_block += 1;
        self.function.blocks.push(IrBlock::new(id, label));
        id
    }

    pub fn switch_to(&mut self, block: BlockId) {
        self.current_block = block;
    }

    pub fn current_block_mut(&mut self) -> &mut IrBlock {
        let id = self.current_block as usize;
        &mut self.function.blocks[id]
    }

    pub fn emit(&mut self, instr: IrInstr) {
        self.current_block_mut().instrs.push(instr);
    }

    pub fn terminate(&mut self, term: IrTerminator) {
        self.current_block_mut().terminator = Some(term);
    }

    pub fn is_terminated(&self) -> bool {
        self.function.blocks[self.current_block as usize].terminator.is_some()
    }

    // ── Convenience emitters ──────────────────────────────────────────────

    pub fn const_i64(&mut self, v: i64) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::ConstI64(r, v));
        r
    }

    pub fn const_f64(&mut self, v: f64) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::ConstF64(r, v));
        r
    }

    pub fn str_ptr(&mut self, s: impl Into<String>) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::StrPtr(r, s.into()));
        r
    }

    pub fn alloca(&mut self, ty: IrType) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::Alloca(r, ty));
        r
    }

    pub fn store(&mut self, val: ValId, ptr: ValId) {
        self.emit(IrInstr::Store(val, ptr));
    }

    pub fn load(&mut self, ptr: ValId, ty: IrType) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::Load(r, ptr, ty));
        r
    }

    pub fn add(&mut self, a: ValId, b: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::Add(r, a, b));
        r
    }

    pub fn sub(&mut self, a: ValId, b: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::Sub(r, a, b));
        r
    }

    pub fn mul(&mut self, a: ValId, b: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::Mul(r, a, b));
        r
    }

    pub fn div(&mut self, a: ValId, b: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::Div(r, a, b));
        r
    }

    pub fn rem(&mut self, a: ValId, b: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::Rem(r, a, b));
        r
    }

    pub fn fadd(&mut self, a: ValId, b: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::FAdd(r, a, b));
        r
    }

    pub fn fsub(&mut self, a: ValId, b: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::FSub(r, a, b));
        r
    }

    pub fn fmul(&mut self, a: ValId, b: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::FMul(r, a, b));
        r
    }

    pub fn fdiv(&mut self, a: ValId, b: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::FDiv(r, a, b));
        r
    }

    pub fn cmp(&mut self, op: CmpOp, a: ValId, b: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::Cmp(r, op, a, b));
        r
    }

    pub fn and(&mut self, a: ValId, b: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::And(r, a, b));
        r
    }

    pub fn or(&mut self, a: ValId, b: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::Or(r, a, b));
        r
    }

    pub fn not(&mut self, a: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::Not(r, a));
        r
    }

    pub fn call(&mut self, name: impl Into<String>, args: Vec<ValId>) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::Call(r, name.into(), args));
        r
    }

    pub fn syscall(&mut self, num: ValId, args: Vec<ValId>) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::SysCall(r, num, args));
        r
    }

    pub fn gep(&mut self, ptr: ValId, idx: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::Gep(r, ptr, idx));
        r
    }

    pub fn zext(&mut self, a: ValId) -> ValId {
        let r = self.fresh_val();
        self.emit(IrInstr::ZExt(r, a));
        r
    }

    pub fn build(self) -> IrFunction {
        self.function
    }
}
