/// Vajra x86-64 Code Generator + Instruction Encoder
/// Compiles Vajra IR → x86-64 machine code stored in COFF/ELF object format.
/// Uses the `object` crate (pure Rust) to write the object file.
/// NO LLVM, NO MSVC, NO GCC — 100% self-contained.

use std::collections::HashMap;
use object::write::{Object, StandardSection, Symbol, SymbolSection, Relocation};
use object::{Architecture, BinaryFormat, Endianness, SymbolKind, SymbolScope, RelocationEncoding, RelocationFlags};
use anyhow::Result;
use crate::ir::*;

// ─── Register Definitions ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reg {
    Rax, Rcx, Rdx, Rbx, Rsp, Rbp, Rsi, Rdi,
    R8, R9, R10, R11, R12, R13, R14, R15,
    Xmm0, Xmm1, Xmm2, Xmm3, Xmm4, Xmm5, Xmm6, Xmm7,
}

impl Reg {
    pub fn num(&self) -> u8 {
        match self {
            Reg::Rax => 0, Reg::Rcx => 1, Reg::Rdx => 2, Reg::Rbx => 3,
            Reg::Rsp => 4, Reg::Rbp => 5, Reg::Rsi => 6, Reg::Rdi => 7,
            Reg::R8  => 8, Reg::R9  => 9, Reg::R10 =>10, Reg::R11 =>11,
            Reg::R12 =>12, Reg::R13 =>13, Reg::R14 =>14, Reg::R15 =>15,
            Reg::Xmm0 => 0, Reg::Xmm1 => 1, Reg::Xmm2 => 2, Reg::Xmm3 => 3,
            Reg::Xmm4 => 4, Reg::Xmm5 => 5, Reg::Xmm6 => 6, Reg::Xmm7 => 7,
        }
    }
    pub fn is_xmm(&self) -> bool {
        matches!(self, Reg::Xmm0 | Reg::Xmm1 | Reg::Xmm2 | Reg::Xmm3
            | Reg::Xmm4 | Reg::Xmm5 | Reg::Xmm6 | Reg::Xmm7)
    }
    pub fn needs_rex(&self) -> bool {
        self.num() >= 8 && !self.is_xmm()
    }
}

// ─── Instruction Encoders (inline) ───────────────────────────────────────────

fn emit_mov_imm64(code: &mut Vec<u8>, reg: Reg, imm: i64) {
    let rnum = reg.num();
    if reg.needs_rex() {
        code.push(0x49);
        code.push(0xB8 | (rnum & 7));
    } else {
        code.push(0x48);
        code.push(0xB8 | rnum);
    }
    code.extend_from_slice(&imm.to_le_bytes());
}

fn emit_load_rbp_offset(code: &mut Vec<u8>, reg: Reg, off: i32) {
    let rnum = reg.num();
    let rex = if reg.needs_rex() { 0x4Cu8 } else { 0x48 };
    code.push(rex);
    code.push(0x8B);
    if off >= -128 && off <= 127 {
        code.push(0x45 | ((rnum & 7) << 3));
        code.push(off as i8 as u8);
    } else {
        code.push(0x85 | ((rnum & 7) << 3));
        code.extend_from_slice(&off.to_le_bytes());
    }
}

fn emit_store_rbp_offset(code: &mut Vec<u8>, off: i32, reg: Reg) {
    let rnum = reg.num();
    let rex = if reg.needs_rex() { 0x4Cu8 } else { 0x48 };
    code.push(rex);
    code.push(0x89);
    if off >= -128 && off <= 127 {
        code.push(0x45 | ((rnum & 7) << 3));
        code.push(off as i8 as u8);
    } else {
        code.push(0x85 | ((rnum & 7) << 3));
        code.extend_from_slice(&off.to_le_bytes());
    }
}

fn emit_lea_rbp_offset(code: &mut Vec<u8>, reg: Reg, off: i32) {
    let rnum = reg.num();
    let rex = if reg.needs_rex() { 0x4Cu8 } else { 0x48 };
    code.push(rex);
    code.push(0x8D);
    if off >= -128 && off <= 127 {
        code.push(0x45 | ((rnum & 7) << 3));
        code.push(off as i8 as u8);
    } else {
        code.push(0x85 | ((rnum & 7) << 3));
        code.extend_from_slice(&off.to_le_bytes());
    }
}

fn emit_mov_reg_reg(code: &mut Vec<u8>, dst: Reg, src: Reg) {
    let dnum = dst.num();
    let snum = src.num();
    let mut rex = 0x48u8;
    if dst.needs_rex() { rex |= 0x01; }
    if src.needs_rex() { rex |= 0x04; }
    code.push(rex);
    code.push(0x89);
    code.push(0xC0 | ((snum & 7) << 3) | (dnum & 7));
}

fn emit_lea_rip_rel(code: &mut Vec<u8>, reg: Reg) {
    let rnum = reg.num();
    let rex = if reg.needs_rex() { 0x4Cu8 } else { 0x48 };
    code.push(rex);
    code.push(0x8D);
    code.push(0x05 | ((rnum & 7) << 3));
}

fn emit_movsd_xmm_from_mem(code: &mut Vec<u8>, xmm: Reg, off: i32) {
    let xnum = xmm.num() & 7;
    code.push(0xF2);
    code.push(0x0F);
    code.push(0x10);
    if off >= -128 && off <= 127 {
        code.push(0x45 | (xnum << 3));
        code.push(off as i8 as u8);
    } else {
        code.push(0x85 | (xnum << 3));
        code.extend_from_slice(&off.to_le_bytes());
    }
}

fn emit_movq_xmm_to_mem(code: &mut Vec<u8>, xmm: Reg, off: i32) {
    let xnum = xmm.num() & 7;
    code.push(0x66);
    code.push(0x0F);
    code.push(0xD6);
    if off >= -128 && off <= 127 {
        code.push(0x45 | (xnum << 3));
        code.push(off as i8 as u8);
    } else {
        code.push(0x85 | (xnum << 3));
        code.extend_from_slice(&off.to_le_bytes());
    }
}

fn align16(n: u32) -> u32 {
    (n + 15) & !15
}

fn get_arg_regs() -> &'static [Reg] {
    #[cfg(target_os = "windows")]
    { &[Reg::Rcx, Reg::Rdx, Reg::R8, Reg::R9] }
    #[cfg(not(target_os = "windows"))]
    { &[Reg::Rdi, Reg::Rsi, Reg::Rdx, Reg::Rcx, Reg::R8, Reg::R9] }
}

// ─── Value Location ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum ValueLoc {
    Reg(Reg),
    Stack(i32),
    Imm(i64),
    Global(String),
}

struct PendingReloc {
    offset: u64,
    symbol: String,
    addend: i64,
}

// ─── Per-function state ───────────────────────────────────────────────────────

struct FuncGen {
    code: Vec<u8>,
    val_locs: HashMap<ValId, ValueLoc>,
    stack_offset: i32,
    relocs: Vec<PendingReloc>,
    block_offsets: HashMap<BlockId, usize>,
    pending_branches: Vec<(usize, BlockId)>,
    alloca_slots: std::collections::HashSet<ValId>,
    reg_val: HashMap<Reg, ValId>,
    saved_regs: Vec<(Reg, i32)>,
    reg_assignment: HashMap<ValId, Reg>,
    non_spillable: std::collections::HashSet<ValId>,
}

impl FuncGen {
    fn new() -> Self {
        Self {
            code: Vec::new(),
            val_locs: HashMap::new(),
            stack_offset: 0,
            relocs: Vec::new(),
            block_offsets: HashMap::new(),
            pending_branches: Vec::new(),
            alloca_slots: std::collections::HashSet::new(),
            reg_val: HashMap::new(),
            saved_regs: Vec::new(),
            reg_assignment: HashMap::new(),
            non_spillable: std::collections::HashSet::new(),
        }
    }
    fn set_reg(&mut self, reg: Reg, val_id: ValId) {
        if reg.is_xmm() { return; }
        self.reg_val.retain(|&r, &mut v| v != val_id || r.is_xmm());
        self.reg_val.insert(reg, val_id);
    }
    fn invalidate_reg(&mut self, reg: Reg) {
        if reg.is_xmm() { return; }
        self.reg_val.remove(&reg);
    }
    fn clear_cache(&mut self) {
        self.reg_val.clear();
    }
    fn alloc_stack(&mut self, _size: i32) -> i32 {
        self.stack_offset -= 8;
        self.stack_offset
    }
    fn emit(&mut self, bytes: &[u8]) { self.code.extend_from_slice(bytes); }
    fn pos(&self) -> usize { self.code.len() }
}

fn analyze_and_assign_regs(func: &IrFunction) -> std::collections::HashMap<ValId, Reg> {
    use std::collections::{HashMap, HashSet};

    let mut allocas = HashSet::new();
    for block in &func.blocks {
        for instr in &block.instrs {
            if let IrInstr::Alloca(dst, _) = instr {
                allocas.insert(*dst);
            }
        }
    }

    let mut alloca_uses: HashMap<ValId, usize> = HashMap::new();
    let mut address_taken = HashSet::new();

    for block in &func.blocks {
        for instr in &block.instrs {
            match instr {
                IrInstr::Load(_dst, ptr, _) => {
                    if allocas.contains(ptr) {
                        *alloca_uses.entry(*ptr).or_insert(0) += 1;
                    }
                }
                IrInstr::Store(val, ptr) => {
                    if allocas.contains(ptr) {
                        *alloca_uses.entry(*ptr).or_insert(0) += 1;
                    }
                    if allocas.contains(val) {
                        address_taken.insert(*val);
                    }
                }
                other => {
                    let mut referenced = Vec::new();
                    match other {
                        IrInstr::ConstI64(dst, _) |
                        IrInstr::ConstF64(dst, _) |
                        IrInstr::ConstBool(dst, _) |
                        IrInstr::StrPtr(dst, _) => { referenced.push(*dst); }
                        IrInstr::Alloca(dst, _) => { referenced.push(*dst); }
                        IrInstr::Add(dst, a, b) |
                        IrInstr::Sub(dst, a, b) |
                        IrInstr::Mul(dst, a, b) |
                        IrInstr::Div(dst, a, b) |
                        IrInstr::Rem(dst, a, b) |
                        IrInstr::FAdd(dst, a, b) |
                        IrInstr::FSub(dst, a, b) |
                        IrInstr::FMul(dst, a, b) |
                        IrInstr::FDiv(dst, a, b) |
                        IrInstr::Cmp(dst, _, a, b) |
                        IrInstr::And(dst, a, b) |
                        IrInstr::Or(dst, a, b) => {
                            referenced.push(*dst);
                            referenced.push(*a);
                            referenced.push(*b);
                        }
                        IrInstr::Neg(dst, a) |
                        IrInstr::Not(dst, a) |
                        IrInstr::ItoF(dst, a) |
                        IrInstr::FtoI(dst, a) |
                        IrInstr::ZExt(dst, a) |
                        IrInstr::SExt(dst, a) |
                        IrInstr::Trunc(dst, a, _) |
                        IrInstr::BitCast(dst, a, _) => {
                            referenced.push(*dst);
                            referenced.push(*a);
                        }
                        IrInstr::Gep(dst, ptr, idx) => {
                            referenced.push(*dst);
                            referenced.push(*ptr);
                            referenced.push(*idx);
                        }
                        IrInstr::AtomicAdd(ptr, val) => {
                            referenced.push(*ptr);
                            referenced.push(*val);
                        }
                        IrInstr::Call(dst, _, args) |
                        IrInstr::CallIndirect(dst, _, args) |
                        IrInstr::SysCall(dst, _, args) => {
                            referenced.push(*dst);
                            referenced.extend(args.iter().copied());
                        }
                        IrInstr::Phi(dst, incoming) => {
                            referenced.push(*dst);
                            for &(v, _) in incoming {
                                referenced.push(v);
                            }
                        }
                        IrInstr::Load(_, _, _) | IrInstr::Store(_, _) => {}
                        IrInstr::Comment(_) => {}
                    }
                    for v in referenced {
                        if allocas.contains(&v) {
                            address_taken.insert(v);
                        }
                    }
                }
            }
        }
        if let Some(term) = &block.terminator {
            let mut referenced = Vec::new();
            match term {
                IrTerminator::Ret(val) => { referenced.push(*val); }
                IrTerminator::Jump(_) => {}
                IrTerminator::Branch(cond, _, _) => { referenced.push(*cond); }
                IrTerminator::Unreachable => {}
            }
            for v in referenced {
                if allocas.contains(&v) {
                    address_taken.insert(v);
                }
            }
        }
    }

    let mut candidates: Vec<(ValId, usize)> = alloca_uses.into_iter()
        .filter(|(val, _)| !address_taken.contains(val))
        .collect();

    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    // Available callee-saved registers: R12, R13, R14, R15, Rbx, Rsi, Rdi
    let regs = [Reg::R12, Reg::R13, Reg::R14, Reg::R15, Reg::Rbx, Reg::Rsi, Reg::Rdi];
    let mut assignment = HashMap::new();
    for (i, (val, _)) in candidates.iter().take(regs.len()).enumerate() {
        assignment.insert(*val, regs[i]);
    }
    assignment
}

// ─── Codegen ─────────────────────────────────────────────────────────────────

pub fn compile(module: &IrModule) -> Result<Vec<u8>> {
    let mut gen = X86_64Codegen::new(module);
    gen.compile_module()
}

struct X86_64Codegen<'m> {
    module: &'m IrModule,
    global_offsets: HashMap<String, u64>,
}

impl<'m> X86_64Codegen<'m> {
    pub fn new(module: &'m IrModule) -> Self {
        Self { module, global_offsets: HashMap::new() }
    }

    fn is_float_returning(&self, name: &str) -> bool {
        matches!(
            name,
            "vajra_math_sin"
                | "vajra_math_cos"
                | "vajra_math_tan"
                | "vajra_math_sqrt"
                | "vajra_math_abs"
                | "vajra_math_log"
                | "vajra_math_pow"
                | "vajra_random_float"
        )
    }

    fn load_operands(&self, fg: &mut FuncGen, a: ValId, b: ValId) -> Result<()> {
        let b_in_rax = fg.reg_val.get(&Reg::Rax) == Some(&b) || 
            matches!(fg.val_locs.get(&b), Some(ValueLoc::Reg(Reg::Rax)));
        if b_in_rax {
            self.load_into(fg, b, Reg::Rcx)?;
            self.load_into(fg, a, Reg::Rax)?;
        } else {
            self.load_into(fg, a, Reg::Rax)?;
            self.load_into(fg, b, Reg::Rcx)?;
        }
        Ok(())
    }

    pub fn compile_module(&mut self) -> Result<Vec<u8>> {
        let (binary_format, arch) = if cfg!(target_os = "windows") {
            (BinaryFormat::Coff, Architecture::X86_64)
        } else {
            (BinaryFormat::Elf, Architecture::X86_64)
        };

        let mut obj = Object::new(binary_format, arch, Endianness::Little);

        let rdata_section = obj.section_id(StandardSection::ReadOnlyData);
        let data_section = obj.section_id(StandardSection::Data);
        for global in &self.module.globals {
            let section = if global.name == "vajra_global_instance" {
                data_section
            } else {
                rdata_section
            };
            let offset = obj.append_section_data(section, &global.data, 8);
            self.global_offsets.insert(global.name.clone(), offset);
            obj.add_symbol(Symbol {
                name: global.name.as_bytes().to_vec(),
                value: offset,
                size: global.data.len() as u64,
                kind: SymbolKind::Data,
                scope: SymbolScope::Compilation,
                weak: false,
                section: SymbolSection::Section(section),
                flags: object::SymbolFlags::None,
            });
        }

        let text_section = obj.section_id(StandardSection::Text);

        // Standard runtime externs
        let runtime_syms = [
            "vajra_runtime_init", "vajra_print_i64", "vajra_print_f64",
            "vajra_print_str", "vajra_print_auto", "vajra_alloc", "vajra_free",
            "vajra_exit", "vajra_throw", "vajra_spawn", "vajra_spawn_val",
            "vajra_strlen", "vajra_readline", "printf",
        ];
        let mut extern_sym_ids = HashMap::new();
        for &sym in &runtime_syms {
            let sym_id = obj.add_symbol(Symbol {
                name: sym.as_bytes().to_vec(),
                value: 0, size: 0,
                kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
                weak: false, section: SymbolSection::Undefined,
                flags: object::SymbolFlags::None,
            });
            extern_sym_ids.insert(sym.to_string(), sym_id);
        }
        for name in &self.module.extern_functions {
            if !extern_sym_ids.contains_key(name) {
                let sym_id = obj.add_symbol(Symbol {
                    name: name.as_bytes().to_vec(),
                    value: 0, size: 0,
                    kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
                    weak: false, section: SymbolSection::Undefined,
                    flags: object::SymbolFlags::None,
                });
                extern_sym_ids.insert(name.clone(), sym_id);
            }
        }

        let self_ref = &*self;
        let functions: Vec<_> = self.module.functions.iter().filter(|f| !f.is_extern).collect();
        let compiled_funcs: Vec<(String, Result<FuncGen>, bool)> = std::thread::scope(|s| {
            let mut handles = Vec::new();
            for func in &functions {
                let func_ref = *func;
                handles.push(s.spawn(move || {
                    (func_ref.name.clone(), self_ref.compile_function(func_ref), func_ref.is_main)
                }));
            }
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        for (name, func_code_res, is_main) in compiled_funcs {
            let func_code = func_code_res?;
            let func_offset = obj.append_section_data(text_section, &func_code.code, 16);

            let func_sym_id = obj.add_symbol(Symbol {
                name: name.as_bytes().to_vec(),
                value: func_offset,
                size: func_code.code.len() as u64,
                kind: SymbolKind::Text,
                scope: if is_main { SymbolScope::Dynamic } else { SymbolScope::Compilation },
                weak: false,
                section: SymbolSection::Section(text_section),
                flags: object::SymbolFlags::None,
            });

            // Apply relocations
            for reloc in &func_code.relocs {
                let sym_id = if let Some(&id) = extern_sym_ids.get(&reloc.symbol) {
                    id
                } else {
                    obj.add_symbol(Symbol {
                        name: reloc.symbol.as_bytes().to_vec(),
                        value: 0, size: 0,
                        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
                        weak: false, section: SymbolSection::Undefined,
                        flags: object::SymbolFlags::None,
                    })
                };

                obj.add_relocation(
                    text_section,
                    Relocation {
                        offset: func_offset + reloc.offset,
                        symbol: sym_id,
                        addend: reloc.addend,
                        flags: RelocationFlags::Generic {
                            kind: object::RelocationKind::Relative,
                            encoding: RelocationEncoding::Generic,
                            size: 32,
                        },
                    },
                ).map_err(|e| anyhow::anyhow!("Relocation error: {}", e))?;
            }
            let _ = func_sym_id;
        }

        obj.write().map_err(|e| anyhow::anyhow!("Object write error: {}", e))
    }

    fn compile_function(&self, func: &IrFunction) -> Result<FuncGen> {
        let mut fg = FuncGen::new();

        let use_counts = get_use_counts(func);
        let mut non_spillable = std::collections::HashSet::new();
        for block in &func.blocks {
            let mut i = 0;
            while i < block.instrs.len() {
                let instr = &block.instrs[i];
                if let Some(dst) = get_instr_dst(instr) {
                    let mut next_idx = i + 1;
                    while next_idx < block.instrs.len() {
                        let next_instr = &block.instrs[next_idx];
                        if !matches!(next_instr, IrInstr::Alloca(_, _) | IrInstr::Comment(_)) {
                            if is_user_of(next_instr, dst) && use_counts.get(&dst).cloned().unwrap_or(0) == 1 {
                                non_spillable.insert(dst);
                            }
                            break;
                        }
                        next_idx += 1;
                    }
                }
                i += 1;
            }
        }
        fg.non_spillable = non_spillable;

        // Prologue: push rbp; mov rbp,rsp
        fg.emit(&[0x55, 0x48, 0x89, 0xE5]);
        // sub rsp, frame_size (placeholder — patched later)
        let frame_patch = fg.pos();
        fg.emit(&[0x48, 0x81, 0xEC, 0x00, 0x00, 0x00, 0x00]);

        // Analyze and assign registers for alloca slots
        let reg_assignment = analyze_and_assign_regs(func);
        fg.reg_assignment = reg_assignment.clone();

        // Save callee-saved registers that we will use
        for &reg in reg_assignment.values() {
            let slot = fg.alloc_stack(8);
            emit_store_rbp_offset(&mut fg.code, slot, reg);
            fg.saved_regs.push((reg, slot));
        }

        // Map parameters to argument registers
        let arg_regs = get_arg_regs();
        for (i, param) in func.params.iter().enumerate() {
            if i < arg_regs.len() {
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, arg_regs[i]);
                fg.val_locs.insert(param.val, ValueLoc::Stack(off));
                fg.set_reg(arg_regs[i], param.val);
            }
        }

        // Compile blocks
        for block in &func.blocks {
            fg.clear_cache();
            let block_pos = fg.pos();
            fg.block_offsets.insert(block.id, block_pos);

            // Patch any pending forward branches to this block
            let mut i = 0;
            while i < fg.pending_branches.len() {
                if fg.pending_branches[i].1 == block.id {
                    let patch = fg.pending_branches[i].0;
                    let rel = (block_pos as i64 - patch as i64 - 4) as i32;
                    fg.code[patch..patch+4].copy_from_slice(&rel.to_le_bytes());
                    fg.pending_branches.remove(i);
                } else { i += 1; }
            }

            for instr in &block.instrs {
                self.compile_instr(instr, &mut fg)?;
            }
            if let Some(term) = &block.terminator {
                self.compile_term(term, &mut fg)?;
            }
        }

        // Patch remaining forward branches (to end of function)
        let end_pos = fg.pos();
        for (patch, _) in &fg.pending_branches {
            let rel = (end_pos as i64 - *patch as i64 - 4) as i32;
            fg.code[*patch..*patch+4].copy_from_slice(&rel.to_le_bytes());
        }

        // Patch frame size — must be aligned so RSP is 16-byte aligned at CALL sites.
        // After `push rbp` (-8 bytes) + `sub rsp, frame`, the stack offset from original is -(8 + frame).
        // At each CALL, RSP must be 16-aligned.
        // Since `push rbp` pushes 8 bytes, RSP at entry was `16n + 8`. After `push rbp` it becomes `16n`.
        // So `frame` must be a multiple of 16 (i.e., frame ≡ 0 (mod 16)) to keep RSP 16-aligned.
        let raw = (-fg.stack_offset).max(32) as u32;
        let frame = align16(raw); // round up to multiple of 16
        fg.code[frame_patch+3..frame_patch+7].copy_from_slice(&frame.to_le_bytes());

        Ok(fg)
    }

    fn compile_instr(&self, instr: &IrInstr, fg: &mut FuncGen) -> Result<()> {
        match instr {
            IrInstr::ConstI64(dst, val) => { fg.val_locs.insert(*dst, ValueLoc::Imm(*val)); }
            IrInstr::ConstF64(dst, val) => { fg.val_locs.insert(*dst, ValueLoc::Imm(val.to_bits() as i64)); }
            IrInstr::ConstBool(dst, val) => { fg.val_locs.insert(*dst, ValueLoc::Imm(if *val { 1 } else { 0 })); }
            IrInstr::StrPtr(dst, name) => { fg.val_locs.insert(*dst, ValueLoc::Global(name.clone())); }
            IrInstr::Alloca(dst, _) => {
                fg.alloca_slots.insert(*dst);
                if let Some(&reg) = fg.reg_assignment.get(dst) {
                    fg.val_locs.insert(*dst, ValueLoc::Reg(reg));
                } else {
                    let off = fg.alloc_stack(8);
                    fg.val_locs.insert(*dst, ValueLoc::Stack(off));
                }
            }
            IrInstr::Store(val_id, ptr_id) => {
                if fg.alloca_slots.contains(ptr_id) {
                    let ptr_loc = fg.val_locs.get(ptr_id).cloned()
                        .ok_or_else(|| anyhow::anyhow!("store: unknown ptr {}", ptr_id))?;
                    match ptr_loc {
                        ValueLoc::Stack(off) => {
                            self.load_into(fg, *val_id, Reg::Rax)?;
                            emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                        }
                        ValueLoc::Reg(reg) => {
                            self.load_into(fg, *val_id, reg)?;
                        }
                        _ => {}
                    }
                } else {
                    self.load_operands(fg, *val_id, *ptr_id)?;
                    fg.code.extend_from_slice(&[0x48, 0x89, 0x01]); // mov [rcx], rax
                }
            }

            IrInstr::Load(dst, ptr_id, _) => {
                if fg.alloca_slots.contains(ptr_id) {
                    let ptr_loc = fg.val_locs.get(ptr_id).cloned()
                        .ok_or_else(|| anyhow::anyhow!("load: unknown ptr {}", ptr_id))?;
                    match ptr_loc {
                        ValueLoc::Stack(ptr_off) => {
                            fg.val_locs.insert(*dst, ValueLoc::Stack(ptr_off));
                        }
                        ValueLoc::Reg(reg) => {
                            fg.val_locs.insert(*dst, ValueLoc::Reg(reg));
                        }
                        _ => {}
                    }
                } else {
                    self.load_into(fg, *ptr_id, Reg::Rax)?;
                    fg.code.extend_from_slice(&[0x48, 0x8B, 0x00]); // mov rax, [rax]
                    fg.invalidate_reg(Reg::Rax);
                    let off = fg.alloc_stack(8);
                    emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                    fg.val_locs.insert(*dst, ValueLoc::Stack(off));
                    fg.set_reg(Reg::Rax, *dst);
                }
            }

            IrInstr::Add(dst, a, b) => { self.call_binary_helper(fg, *dst, *a, *b, "vajra_add")?; }
            IrInstr::Sub(dst, a, b) => { self.call_binary_helper(fg, *dst, *a, *b, "vajra_sub")?; }
            IrInstr::Mul(dst, a, b) => { self.call_binary_helper(fg, *dst, *a, *b, "vajra_mul")?; }
            IrInstr::Div(dst, a, b) => { self.call_binary_helper(fg, *dst, *a, *b, "vajra_div")?; }
            IrInstr::Rem(dst, a, b) => { self.call_binary_helper(fg, *dst, *a, *b, "vajra_rem")?; }
            IrInstr::Neg(dst, a) => {
                let arg_regs = get_arg_regs();
                emit_mov_imm64(&mut fg.code, arg_regs[0], 1); // tagged 0
                self.load_into(fg, *a, arg_regs[1])?;
                fg.clear_cache();
                #[cfg(target_os = "windows")]
                fg.code.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]); // sub rsp, 32
                fg.code.push(0xE8);
                let patch = fg.pos();
                fg.code.extend_from_slice(&[0; 4]);
                fg.relocs.push(PendingReloc { offset: patch as u64, symbol: "vajra_sub".to_string(), addend: -4 });
                #[cfg(target_os = "windows")]
                fg.code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20]); // add rsp, 32
                
                fg.invalidate_reg(Reg::Rax);
                if fg.non_spillable.contains(dst) {
                    fg.val_locs.insert(*dst, ValueLoc::Reg(Reg::Rax));
                } else {
                    let off = fg.alloc_stack(8);
                    emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                    fg.val_locs.insert(*dst, ValueLoc::Stack(off));
                }
                fg.set_reg(Reg::Rax, *dst);
            }
            IrInstr::FAdd(dst, a, b) => { self.fop(fg, *dst, *a, *b, &[0xF2, 0x0F, 0x58, 0xC1])?; }
            IrInstr::FSub(dst, a, b) => { self.fop(fg, *dst, *a, *b, &[0xF2, 0x0F, 0x5C, 0xC1])?; }
            IrInstr::FMul(dst, a, b) => { self.fop(fg, *dst, *a, *b, &[0xF2, 0x0F, 0x59, 0xC1])?; }
            IrInstr::FDiv(dst, a, b) => { self.fop(fg, *dst, *a, *b, &[0xF2, 0x0F, 0x5E, 0xC1])?; }
            IrInstr::Cmp(dst, op, a, b) => {
                let arg_regs = get_arg_regs();
                self.load_into(fg, *a, arg_regs[0])?;
                self.load_into(fg, *b, arg_regs[1])?;
                fg.clear_cache();
                #[cfg(target_os = "windows")]
                fg.code.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]); // sub rsp, 32
                fg.code.push(0xE8);
                let patch = fg.pos();
                fg.code.extend_from_slice(&[0; 4]);
                fg.relocs.push(PendingReloc { offset: patch as u64, symbol: "vajra_cmp".to_string(), addend: -4 });
                #[cfg(target_os = "windows")]
                fg.code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20]); // add rsp, 32
                
                fg.code.extend_from_slice(&[0x48, 0x83, 0xF8, 0x00]); // cmp rax, 0
                let setcc: u8 = match op {
                    CmpOp::Eq => 0x94, CmpOp::Ne => 0x95,
                    CmpOp::Lt => 0x9C, CmpOp::Le => 0x9E,
                    CmpOp::Gt => 0x9F, CmpOp::Ge => 0x9D,
                };
                fg.code.extend_from_slice(&[0x0F, setcc, 0xC0]); // setCC al
                fg.code.extend_from_slice(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
                fg.code.extend_from_slice(&[0x48, 0xD1, 0xE0, 0x48, 0x83, 0xC0, 0x01]); // shl rax, 1; add rax, 1 (tag boolean)
                
                fg.invalidate_reg(Reg::Rax);
                if fg.non_spillable.contains(dst) {
                    fg.val_locs.insert(*dst, ValueLoc::Reg(Reg::Rax));
                } else {
                    let off = fg.alloc_stack(8);
                    emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                    fg.val_locs.insert(*dst, ValueLoc::Stack(off));
                }
                fg.set_reg(Reg::Rax, *dst);
            }
            IrInstr::And(dst, a, b) => { self.call_binary_helper(fg, *dst, *a, *b, "vajra_and")?; }
            IrInstr::Or(dst, a, b)  => { self.call_binary_helper(fg, *dst, *a, *b, "vajra_or")?; }
            IrInstr::Not(dst, a) => {
                let arg_regs = get_arg_regs();
                self.load_into(fg, *a, arg_regs[0])?;
                fg.clear_cache();
                #[cfg(target_os = "windows")]
                fg.code.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]); // sub rsp, 32
                fg.code.push(0xE8);
                let patch = fg.pos();
                fg.code.extend_from_slice(&[0; 4]);
                fg.relocs.push(PendingReloc { offset: patch as u64, symbol: "vajra_not".to_string(), addend: -4 });
                #[cfg(target_os = "windows")]
                fg.code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20]); // add rsp, 32
                
                fg.invalidate_reg(Reg::Rax);
                if fg.non_spillable.contains(dst) {
                    fg.val_locs.insert(*dst, ValueLoc::Reg(Reg::Rax));
                } else {
                    let off = fg.alloc_stack(8);
                    emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                    fg.val_locs.insert(*dst, ValueLoc::Stack(off));
                }
                fg.set_reg(Reg::Rax, *dst);
            }
            IrInstr::Call(dst, name, args) => {
                let is_float_ret = self.is_float_returning(name);
                
                // Load arguments based on float type
                if name == "vajra_math_pow" {
                    if args.len() >= 2 {
                        self.load_f64_into(fg, args[0], Reg::Xmm0)?;
                        self.load_f64_into(fg, args[1], Reg::Xmm1)?;
                    }
                } else if name == "vajra_math_sin"
                    || name == "vajra_math_cos"
                    || name == "vajra_math_tan"
                    || name == "vajra_math_sqrt"
                    || name == "vajra_math_abs"
                    || name == "vajra_math_log"
                    || name == "vajra_print_f64"
                {
                    if args.len() >= 1 {
                        self.load_f64_into(fg, args[0], Reg::Xmm0)?;
                    }
                } else {
                    let arg_regs = get_arg_regs();
                    for (i, &arg_id) in args.iter().enumerate() {
                        if i < arg_regs.len() {
                            self.load_into(fg, arg_id, arg_regs[i])?;
                        }
                    }
                }
                
                fg.clear_cache();
                #[cfg(target_os = "windows")]
                fg.code.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]); // sub rsp, 32 (shadow space)
                fg.code.push(0xE8);
                let patch = fg.pos();
                fg.code.extend_from_slice(&[0; 4]);
                fg.relocs.push(PendingReloc { offset: patch as u64, symbol: name.clone(), addend: -4 });
                #[cfg(target_os = "windows")]
                fg.code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20]); // add rsp, 32
                
                let off = fg.alloc_stack(8);
                if is_float_ret {
                    emit_movq_xmm_to_mem(&mut fg.code, Reg::Xmm0, off);
                    fg.val_locs.insert(*dst, ValueLoc::Stack(off));
                } else {
                    emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                    fg.val_locs.insert(*dst, ValueLoc::Stack(off));
                    fg.set_reg(Reg::Rax, *dst);
                }
            }
            IrInstr::ZExt(dst, src) | IrInstr::SExt(dst, src) | IrInstr::Trunc(dst, src, _) | IrInstr::BitCast(dst, src, _) => {
                self.load_into(fg, *src, Reg::Rax)?;
                fg.invalidate_reg(Reg::Rax);
                if fg.non_spillable.contains(dst) {
                    fg.val_locs.insert(*dst, ValueLoc::Reg(Reg::Rax));
                } else {
                    let off = fg.alloc_stack(8);
                    emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                    fg.val_locs.insert(*dst, ValueLoc::Stack(off));
                }
                fg.set_reg(Reg::Rax, *dst);
            }
            IrInstr::ItoF(dst, src) => {
                self.load_into(fg, *src, Reg::Rax)?;
                fg.code.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2A, 0xC0]); // cvtsi2sd xmm0, rax
                let off = fg.alloc_stack(8);
                emit_movq_xmm_to_mem(&mut fg.code, Reg::Xmm0, off);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::FtoI(dst, src) => {
                self.load_f64_into(fg, *src, Reg::Xmm0)?;
                fg.code.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2C, 0xC0]); // cvttsd2si rax, xmm0
                fg.invalidate_reg(Reg::Rax);
                if fg.non_spillable.contains(dst) {
                    fg.val_locs.insert(*dst, ValueLoc::Reg(Reg::Rax));
                } else {
                    let off = fg.alloc_stack(8);
                    emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                    fg.val_locs.insert(*dst, ValueLoc::Stack(off));
                }
                fg.set_reg(Reg::Rax, *dst);
            }
            IrInstr::Gep(dst, ptr, idx) => {
                self.load_operands(fg, *ptr, *idx)?;
                fg.code.extend_from_slice(&[0x48, 0x8D, 0x04, 0xC8]); // lea rax, [rax+rcx*8]
                fg.invalidate_reg(Reg::Rax);
                if fg.non_spillable.contains(dst) {
                    fg.val_locs.insert(*dst, ValueLoc::Reg(Reg::Rax));
                } else {
                    let off = fg.alloc_stack(8);
                    emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                    fg.val_locs.insert(*dst, ValueLoc::Stack(off));
                }
                fg.set_reg(Reg::Rax, *dst);
            }
            IrInstr::AtomicAdd(ptr, val) => {
                self.load_operands(fg, *ptr, *val)?;
                fg.invalidate_reg(Reg::Rcx);
                fg.code.extend_from_slice(&[0xF0, 0x48, 0x0F, 0xC1, 0x08]); // lock xadd [rax], rcx
            }
            IrInstr::Phi(dst, _) => {
                let off = fg.alloc_stack(8);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::SysCall(dst, num, args) => {
                self.load_into(fg, *num, Reg::Rax)?;
                let syscall_arg_regs = [Reg::Rdi, Reg::Rsi, Reg::Rdx, Reg::R10, Reg::R8, Reg::R9];
                for (i, &arg) in args.iter().enumerate() {
                    if i < syscall_arg_regs.len() {
                        self.load_into(fg, arg, syscall_arg_regs[i])?;
                    }
                }
                fg.clear_cache();
                fg.code.extend_from_slice(&[0x0F, 0x05]); // syscall
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
                fg.set_reg(Reg::Rax, *dst);
            }
            IrInstr::CallIndirect(dst, func_ptr, args) => {
                let arg_regs = get_arg_regs();
                for (i, &arg) in args.iter().enumerate() {
                    if i < arg_regs.len() { self.load_into(fg, arg, arg_regs[i])?; }
                }
                self.load_into(fg, *func_ptr, Reg::Rax)?;
                fg.clear_cache();
                fg.code.extend_from_slice(&[0xFF, 0xD0]); // call rax
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
                fg.set_reg(Reg::Rax, *dst);
            }

            IrInstr::Comment(_) => {}
        }
        Ok(())
    }

    fn call_binary_helper(&self, fg: &mut FuncGen, dst: ValId, a: ValId, b: ValId, name: &str) -> Result<()> {
        let arg_regs = get_arg_regs();
        self.load_into(fg, a, arg_regs[0])?;
        self.load_into(fg, b, arg_regs[1])?;
        fg.clear_cache();
        #[cfg(target_os = "windows")]
        fg.code.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]); // sub rsp, 32
        fg.code.push(0xE8);
        let patch = fg.pos();
        fg.code.extend_from_slice(&[0; 4]);
        fg.relocs.push(PendingReloc { offset: patch as u64, symbol: name.to_string(), addend: -4 });
        #[cfg(target_os = "windows")]
        fg.code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20]); // add rsp, 32
        
        fg.invalidate_reg(Reg::Rax);
        if fg.non_spillable.contains(&dst) {
            fg.val_locs.insert(dst, ValueLoc::Reg(Reg::Rax));
        } else {
            let off = fg.alloc_stack(8);
            emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
            fg.val_locs.insert(dst, ValueLoc::Stack(off));
        }
        fg.set_reg(Reg::Rax, dst);
        Ok(())
    }

    fn fop(&self, fg: &mut FuncGen, dst: ValId, a: ValId, b: ValId, op_bytes: &[u8]) -> Result<()> {
        self.load_f64_into(fg, a, Reg::Xmm0)?;
        self.load_f64_into(fg, b, Reg::Xmm1)?;
        fg.emit(op_bytes);
        let off = fg.alloc_stack(8);
        emit_movq_xmm_to_mem(&mut fg.code, Reg::Xmm0, off);
        fg.val_locs.insert(dst, ValueLoc::Stack(off));
        Ok(())
    }

    fn compile_term(&self, term: &IrTerminator, fg: &mut FuncGen) -> Result<()> {
        match term {
            IrTerminator::Ret(val) => {
                self.load_into(fg, *val, Reg::Rax)?;
                for &(reg, slot) in fg.saved_regs.iter().rev() {
                    emit_load_rbp_offset(&mut fg.code, reg, slot);
                }
                fg.code.extend_from_slice(&[0xC9, 0xC3]); // leave; ret
            }
            IrTerminator::Jump(target) => {
                if let Some(&toff) = fg.block_offsets.get(target) {
                    let rel = (toff as i64 - fg.pos() as i64 - 5) as i32;
                    fg.code.push(0xE9);
                    fg.code.extend_from_slice(&rel.to_le_bytes());
                } else {
                    fg.code.push(0xE9);
                    let patch = fg.pos();
                    fg.code.extend_from_slice(&[0; 4]);
                    fg.pending_branches.push((patch, *target));
                }
            }
            IrTerminator::Branch(cond, then_b, else_b) => {
                self.load_into(fg, *cond, Reg::Rax)?;
                fg.code.extend_from_slice(&[0x48, 0xD1, 0xF8]); // sar rax, 1
                fg.code.extend_from_slice(&[0x48, 0x85, 0xC0]); // test rax, rax
                fg.code.extend_from_slice(&[0x0F, 0x85]); // jne rel32
                let then_patch = fg.pos();
                fg.code.extend_from_slice(&[0; 4]);
                fg.code.push(0xE9); // jmp rel32 (else)
                let else_patch = fg.pos();
                fg.code.extend_from_slice(&[0; 4]);
                fg.pending_branches.push((then_patch, *then_b));
                fg.pending_branches.push((else_patch, *else_b));
            }
            IrTerminator::Unreachable => {
                fg.code.extend_from_slice(&[0x0F, 0x0B]); // ud2
            }
        }
        Ok(())
    }

    fn load_into(&self, fg: &mut FuncGen, val_id: ValId, reg: Reg) -> Result<()> {
        if reg.is_xmm() {
            let loc = fg.val_locs.get(&val_id).cloned()
                .ok_or_else(|| anyhow::anyhow!("Unknown val {}", val_id))?;
            match loc {
                ValueLoc::Imm(bits) => {
                    emit_mov_imm64(&mut fg.code, Reg::Rax, bits);
                    fg.code.extend_from_slice(&[0x66, 0x48, 0x0F, 0x6E, 0xC0 | (reg.num() << 3)]);
                }
                ValueLoc::Stack(off) => emit_movsd_xmm_from_mem(&mut fg.code, reg, off),
                _ => {
                    self.load_into(fg, val_id, Reg::Rax)?;
                    fg.code.extend_from_slice(&[0x66, 0x48, 0x0F, 0x6E, 0xC0 | (reg.num() << 3)]);
                }
            }
            return Ok(());
        }

        // 1. Check if the value is already in the target register
        if fg.reg_val.get(&reg) == Some(&val_id) {
            return Ok(());
        }

        // 2. Check if the value is in another register
        let mut found_reg = None;
        for (&r, &v) in &fg.reg_val {
            if v == val_id && r != reg {
                found_reg = Some(r);
                break;
            }
        }

        if let Some(src_reg) = found_reg {
            emit_mov_reg_reg(&mut fg.code, reg, src_reg);
            fg.set_reg(reg, val_id);
            return Ok(());
        }

        // 3. Otherwise, load from its primary location
        if fg.alloca_slots.contains(&val_id) {
            let loc = fg.val_locs.get(&val_id).cloned()
                .ok_or_else(|| anyhow::anyhow!("Unknown val {}", val_id))?;
            match loc {
                ValueLoc::Stack(off) => {
                    emit_lea_rbp_offset(&mut fg.code, reg, off);
                    fg.set_reg(reg, val_id);
                    return Ok(());
                }
                ValueLoc::Reg(src) => {
                    if src != reg {
                        emit_mov_reg_reg(&mut fg.code, reg, src);
                    }
                    fg.set_reg(reg, val_id);
                    return Ok(());
                }
                _ => {}
            }
        }

        let loc = fg.val_locs.get(&val_id).cloned()
            .ok_or_else(|| anyhow::anyhow!("Unknown val {}", val_id))?;
        match loc {
            ValueLoc::Imm(v) => {
                emit_mov_imm64(&mut fg.code, reg, v);
            }
            ValueLoc::Stack(off) => {
                emit_load_rbp_offset(&mut fg.code, reg, off);
            }
            ValueLoc::Reg(src) => {
                if src != reg {
                    emit_mov_reg_reg(&mut fg.code, reg, src);
                }
            }
            ValueLoc::Global(name) => {
                emit_lea_rip_rel(&mut fg.code, reg);
                let patch = fg.pos();
                fg.code.extend_from_slice(&[0; 4]);
                fg.relocs.push(PendingReloc { offset: patch as u64, symbol: name, addend: -4 });
            }
        }

        fg.set_reg(reg, val_id);
        Ok(())
    }

    fn load_f64_into(&self, fg: &mut FuncGen, val_id: ValId, xmm: Reg) -> Result<()> {
        let loc = fg.val_locs.get(&val_id).cloned()
            .ok_or_else(|| anyhow::anyhow!("Unknown val {}", val_id))?;
        match loc {
            ValueLoc::Imm(bits) => {
                emit_mov_imm64(&mut fg.code, Reg::Rax, bits);
                fg.code.extend_from_slice(&[0x66, 0x48, 0x0F, 0x6E, 0xC0 | (xmm.num() << 3)]);
            }
            ValueLoc::Stack(off) => emit_movsd_xmm_from_mem(&mut fg.code, xmm, off),
            _ => { self.load_into(fg, val_id, Reg::Rax)?; fg.code.extend_from_slice(&[0x66, 0x48, 0x0F, 0x6E, 0xC0 | (xmm.num() << 3)]); }
        }
        Ok(())
    }
}

fn get_instr_dst(instr: &IrInstr) -> Option<ValId> {
    match instr {
        IrInstr::Add(dst, _, _) |
        IrInstr::Sub(dst, _, _) |
        IrInstr::Mul(dst, _, _) |
        IrInstr::Div(dst, _, _) |
        IrInstr::Rem(dst, _, _) |
        IrInstr::Neg(dst, _) |
        IrInstr::FAdd(dst, _, _) |
        IrInstr::FSub(dst, _, _) |
        IrInstr::FMul(dst, _, _) |
        IrInstr::FDiv(dst, _, _) |
        IrInstr::Cmp(dst, _, _, _) |
        IrInstr::And(dst, _, _) |
        IrInstr::Or(dst, _, _) |
        IrInstr::Not(dst, _) |
        IrInstr::ZExt(dst, _) |
        IrInstr::SExt(dst, _) |
        IrInstr::Trunc(dst, _, _) |
        IrInstr::BitCast(dst, _, _) |
        IrInstr::ItoF(dst, _) |
        IrInstr::FtoI(dst, _) |
        IrInstr::Gep(dst, _, _) => Some(*dst),
        _ => None,
    }
}

fn is_user_of(instr: &IrInstr, val: ValId) -> bool {
    match instr {
        IrInstr::Store(v, _) => *v == val,
        IrInstr::Add(_, a, b) |
        IrInstr::Sub(_, a, b) |
        IrInstr::Mul(_, a, b) |
        IrInstr::Div(_, a, b) |
        IrInstr::Rem(_, a, b) |
        IrInstr::FAdd(_, a, b) |
        IrInstr::FSub(_, a, b) |
        IrInstr::FMul(_, a, b) |
        IrInstr::FDiv(_, a, b) |
        IrInstr::Cmp(_, _, a, b) |
        IrInstr::And(_, a, b) |
        IrInstr::Or(_, a, b) => *a == val || *b == val,
        IrInstr::Neg(_, a) |
        IrInstr::Not(_, a) |
        IrInstr::ZExt(_, a) |
        IrInstr::SExt(_, a) |
        IrInstr::Trunc(_, a, _) |
        IrInstr::BitCast(_, a, _) |
        IrInstr::ItoF(_, a) |
        IrInstr::FtoI(_, a) => *a == val,
        IrInstr::Gep(_, ptr, idx) => *ptr == val || *idx == val,
        IrInstr::AtomicAdd(ptr, v) => *ptr == val || *v == val,
        _ => false,
    }
}

fn get_use_counts(func: &IrFunction) -> std::collections::HashMap<ValId, usize> {
    let mut counts = std::collections::HashMap::new();
    for block in &func.blocks {
        for instr in &block.instrs {
            let mut refs = Vec::new();
            match instr {
                IrInstr::Store(v, p) => { refs.push(*v); refs.push(*p); }
                IrInstr::Load(_, p, _) => { refs.push(*p); }
                IrInstr::Add(_, a, b) |
                IrInstr::Sub(_, a, b) |
                IrInstr::Mul(_, a, b) |
                IrInstr::Div(_, a, b) |
                IrInstr::Rem(_, a, b) |
                IrInstr::FAdd(_, a, b) |
                IrInstr::FSub(_, a, b) |
                IrInstr::FMul(_, a, b) |
                IrInstr::FDiv(_, a, b) |
                IrInstr::Cmp(_, _, a, b) |
                IrInstr::And(_, a, b) |
                IrInstr::Or(_, a, b) => { refs.push(*a); refs.push(*b); }
                IrInstr::Neg(_, a) |
                IrInstr::Not(_, a) |
                IrInstr::ZExt(_, a) |
                IrInstr::SExt(_, a) |
                IrInstr::Trunc(_, a, _) |
                IrInstr::BitCast(_, a, _) |
                IrInstr::ItoF(_, a) |
                IrInstr::FtoI(_, a) => { refs.push(*a); }
                IrInstr::Gep(_, ptr, idx) => { refs.push(*ptr); refs.push(*idx); }
                IrInstr::AtomicAdd(ptr, val) => { refs.push(*ptr); refs.push(*val); }
                IrInstr::Call(_, _, args) |
                IrInstr::CallIndirect(_, _, args) |
                IrInstr::SysCall(_, _, args) => { refs.extend(args.iter().copied()); }
                IrInstr::Phi(_, incoming) => {
                    for &(v, _) in incoming {
                        refs.push(v);
                    }
                }
                _ => {}
            }
            for r in refs {
                *counts.entry(r).or_insert(0) += 1;
            }
        }
        if let Some(term) = &block.terminator {
            match term {
                IrTerminator::Ret(val) => { *counts.entry(*val).or_insert(0) += 1; }
                IrTerminator::Branch(cond, _, _) => { *counts.entry(*cond).or_insert(0) += 1; }
                _ => {}
            }
        }
    }
    counts
}
