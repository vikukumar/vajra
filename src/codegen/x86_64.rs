/// Vajra x86-64 Code Generator + Instruction Encoder
/// Compiles Vajra IR → x86-64 machine code stored in COFF/ELF object format.
/// Uses the `object` crate (pure Rust) to write the object file.
/// NO LLVM, NO MSVC, NO GCC — 100% self-contained.

use std::collections::HashMap;
use object::write::{Object, StandardSection, Symbol, SymbolSection, Relocation};
use object::{Architecture, BinaryFormat, Endianness, SymbolKind, SymbolScope, RelocationEncoding, RelocationFlags};
use anyhow::{bail, Result};
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
        }
    }
    fn alloc_stack(&mut self, _size: i32) -> i32 {
        self.stack_offset -= 8;
        self.stack_offset
    }
    fn emit(&mut self, bytes: &[u8]) { self.code.extend_from_slice(bytes); }
    fn pos(&self) -> usize { self.code.len() }
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

    pub fn compile_module(&mut self) -> Result<Vec<u8>> {
        let (binary_format, arch) = if cfg!(target_os = "windows") {
            (BinaryFormat::Coff, Architecture::X86_64)
        } else {
            (BinaryFormat::Elf, Architecture::X86_64)
        };

        let mut obj = Object::new(binary_format, arch, Endianness::Little);

        let rdata_section = obj.section_id(StandardSection::ReadOnlyData);
        for global in &self.module.globals {
            let offset = obj.append_section_data(rdata_section, &global.data, 1);
            self.global_offsets.insert(global.name.clone(), offset);
            obj.add_symbol(Symbol {
                name: global.name.as_bytes().to_vec(),
                value: offset,
                size: global.data.len() as u64,
                kind: SymbolKind::Data,
                scope: SymbolScope::Compilation,
                weak: false,
                section: SymbolSection::Section(rdata_section),
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

        for func in &self.module.functions {
            if func.is_extern { continue; }

            let func_code = self.compile_function(func)?;
            let func_offset = obj.append_section_data(text_section, &func_code.code, 16);

            let func_sym_id = obj.add_symbol(Symbol {
                name: func.name.as_bytes().to_vec(),
                value: func_offset,
                size: func_code.code.len() as u64,
                kind: SymbolKind::Text,
                scope: if func.is_main { SymbolScope::Dynamic } else { SymbolScope::Compilation },
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

        // Prologue: push rbp; mov rbp,rsp
        fg.emit(&[0x55, 0x48, 0x89, 0xE5]);
        // sub rsp, frame_size (placeholder — patched later)
        let frame_patch = fg.pos();
        fg.emit(&[0x48, 0x81, 0xEC, 0x00, 0x00, 0x00, 0x00]);

        // Map parameters to argument registers
        let arg_regs = get_arg_regs();
        for (i, param) in func.params.iter().enumerate() {
            if i < arg_regs.len() {
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, arg_regs[i]);
                fg.val_locs.insert(param.val, ValueLoc::Stack(off));
            }
        }

        // Main init call
        if func.is_main {
            // call vajra_runtime_init
            fg.code.push(0xE8);
            let patch = fg.pos();
            fg.code.extend_from_slice(&[0; 4]);
            fg.relocs.push(PendingReloc { offset: patch as u64, symbol: "vajra_runtime_init".into(), addend: -4 });
        }

        // Compile blocks
        for block in &func.blocks {
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

        // Patch frame size
        let frame = align16((-fg.stack_offset).max(32) as u32);
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
                let off = fg.alloc_stack(8);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::Store(val_id, ptr_id) => {
                self.load_into(fg, *val_id, Reg::Rax)?;
                let ptr_loc = fg.val_locs.get(ptr_id).cloned()
                    .ok_or_else(|| anyhow::anyhow!("store: unknown ptr {}", ptr_id))?;
                if let ValueLoc::Stack(off) = ptr_loc {
                    emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                }
            }
            IrInstr::Load(dst, ptr_id, _) => {
                let ptr_loc = fg.val_locs.get(ptr_id).cloned()
                    .ok_or_else(|| anyhow::anyhow!("load: unknown ptr {}", ptr_id))?;
                match ptr_loc {
                    ValueLoc::Stack(ptr_off) => {
                        emit_load_rbp_offset(&mut fg.code, Reg::Rax, ptr_off);
                        fg.code.extend_from_slice(&[0x48, 0x8B, 0x00]); // mov rax, [rax]
                    }
                    ValueLoc::Imm(v) => { emit_mov_imm64(&mut fg.code, Reg::Rax, v); }
                    _ => {}
                }
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::Add(dst, a, b) => { self.arith_op(fg, *dst, *a, *b, &[0x48, 0x01, 0xC8])?; }
            IrInstr::Sub(dst, a, b) => { self.arith_op(fg, *dst, *a, *b, &[0x48, 0x29, 0xC8])?; }
            IrInstr::Mul(dst, a, b) => { self.arith_op(fg, *dst, *a, *b, &[0x48, 0x0F, 0xAF, 0xC1])?; }
            IrInstr::Div(dst, a, b) => {
                self.load_into(fg, *a, Reg::Rax)?;
                self.load_into(fg, *b, Reg::Rcx)?;
                fg.code.extend_from_slice(&[0x48, 0x99, 0x48, 0xF7, 0xF9]); // cqo; idiv rcx
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::Rem(dst, a, b) => {
                self.load_into(fg, *a, Reg::Rax)?;
                self.load_into(fg, *b, Reg::Rcx)?;
                fg.code.extend_from_slice(&[0x48, 0x99, 0x48, 0xF7, 0xF9]); // cqo; idiv rcx
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rdx); // remainder in rdx
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::Neg(dst, a) => {
                self.load_into(fg, *a, Reg::Rax)?;
                fg.code.extend_from_slice(&[0x48, 0xF7, 0xD8]); // neg rax
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::FAdd(dst, a, b) => { self.fop(fg, *dst, *a, *b, &[0xF2, 0x0F, 0x58, 0xC1])?; }
            IrInstr::FSub(dst, a, b) => { self.fop(fg, *dst, *a, *b, &[0xF2, 0x0F, 0x5C, 0xC1])?; }
            IrInstr::FMul(dst, a, b) => { self.fop(fg, *dst, *a, *b, &[0xF2, 0x0F, 0x59, 0xC1])?; }
            IrInstr::FDiv(dst, a, b) => { self.fop(fg, *dst, *a, *b, &[0xF2, 0x0F, 0x5E, 0xC1])?; }
            IrInstr::Cmp(dst, op, a, b) => {
                self.load_into(fg, *a, Reg::Rax)?;
                self.load_into(fg, *b, Reg::Rcx)?;
                fg.code.extend_from_slice(&[0x48, 0x39, 0xC8]); // cmp rax, rcx
                let setcc: u8 = match op {
                    CmpOp::Eq => 0x94, CmpOp::Ne => 0x95,
                    CmpOp::Lt => 0x9C, CmpOp::Le => 0x9E,
                    CmpOp::Gt => 0x9F, CmpOp::Ge => 0x9D,
                };
                fg.code.extend_from_slice(&[0x0F, setcc, 0xC0]); // setCC al
                fg.code.extend_from_slice(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::And(dst, a, b) => { self.arith_op(fg, *dst, *a, *b, &[0x48, 0x21, 0xC8])?; }
            IrInstr::Or(dst, a, b)  => { self.arith_op(fg, *dst, *a, *b, &[0x48, 0x09, 0xC8])?; }
            IrInstr::Not(dst, a) => {
                self.load_into(fg, *a, Reg::Rax)?;
                fg.code.extend_from_slice(&[0x48, 0x85, 0xC0, 0x0F, 0x94, 0xC0, 0x48, 0x0F, 0xB6, 0xC0]); // test rax,rax; sete al; movzx rax,al
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::Call(dst, name, args) => {
                let arg_regs = get_arg_regs();
                for (i, &arg_id) in args.iter().enumerate() {
                    if i < arg_regs.len() {
                        self.load_into(fg, arg_id, arg_regs[i])?;
                    }
                }
                #[cfg(target_os = "windows")]
                fg.code.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]); // sub rsp, 32 (shadow space)
                fg.code.push(0xE8);
                let patch = fg.pos();
                fg.code.extend_from_slice(&[0; 4]);
                fg.relocs.push(PendingReloc { offset: patch as u64, symbol: name.clone(), addend: -4 });
                #[cfg(target_os = "windows")]
                fg.code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20]); // add rsp, 32
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::ZExt(dst, src) | IrInstr::SExt(dst, src) | IrInstr::Trunc(dst, src, _) | IrInstr::BitCast(dst, src, _) => {
                self.load_into(fg, *src, Reg::Rax)?;
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
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
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::Gep(dst, ptr, idx) => {
                self.load_into(fg, *ptr, Reg::Rax)?;
                self.load_into(fg, *idx, Reg::Rcx)?;
                fg.code.extend_from_slice(&[0x48, 0x8D, 0x04, 0xC8]); // lea rax, [rax+rcx*8]
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::AtomicAdd(ptr, val) => {
                self.load_into(fg, *ptr, Reg::Rax)?;
                self.load_into(fg, *val, Reg::Rcx)?;
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
                fg.code.extend_from_slice(&[0x0F, 0x05]); // syscall
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::CallIndirect(dst, func_ptr, args) => {
                let arg_regs = get_arg_regs();
                for (i, &arg) in args.iter().enumerate() {
                    if i < arg_regs.len() { self.load_into(fg, arg, arg_regs[i])?; }
                }
                self.load_into(fg, *func_ptr, Reg::Rax)?;
                fg.code.extend_from_slice(&[0xFF, 0xD0]); // call rax
                let off = fg.alloc_stack(8);
                emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
                fg.val_locs.insert(*dst, ValueLoc::Stack(off));
            }
            IrInstr::Comment(_) => {}
        }
        Ok(())
    }

    fn arith_op(&self, fg: &mut FuncGen, dst: ValId, a: ValId, b: ValId, op_bytes: &[u8]) -> Result<()> {
        self.load_into(fg, a, Reg::Rax)?;
        self.load_into(fg, b, Reg::Rcx)?;
        fg.emit(op_bytes);
        let off = fg.alloc_stack(8);
        emit_store_rbp_offset(&mut fg.code, off, Reg::Rax);
        fg.val_locs.insert(dst, ValueLoc::Stack(off));
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
        let loc = fg.val_locs.get(&val_id).cloned()
            .ok_or_else(|| anyhow::anyhow!("Unknown val {}", val_id))?;
        match loc {
            ValueLoc::Imm(v) => emit_mov_imm64(&mut fg.code, reg, v),
            ValueLoc::Stack(off) => emit_load_rbp_offset(&mut fg.code, reg, off),
            ValueLoc::Reg(src) => {
                if src != reg { emit_mov_reg_reg(&mut fg.code, reg, src); }
            }
            ValueLoc::Global(name) => {
                emit_lea_rip_rel(&mut fg.code, reg);
                let patch = fg.pos();
                fg.code.extend_from_slice(&[0; 4]);
                fg.relocs.push(PendingReloc { offset: patch as u64, symbol: name, addend: -4 });
            }
        }
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
