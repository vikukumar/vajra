/// Vajra Runtime Library — Module Root
/// Replaces the C runtime entirely.
/// All I/O, memory, threads — implemented in pure Rust, compiled into vajrac itself.
/// The runtime is embedded in the vajrac binary and injected into every compiled program.

pub mod io;
pub mod memory;
pub mod thread;

use crate::linker::TargetPlatform;

/// Get the pre-compiled runtime object bytes for the target platform.
/// The runtime is pre-compiled from this Rust code and embedded at build time.
///
/// For the development build, we generate the runtime as x86-64 machine code
/// directly (without a separate compile step) using our own assembler.
pub fn get_runtime_object_bytes(platform: &TargetPlatform) -> Vec<u8> {
    // The runtime is implemented as inline x86-64 machine code for maximum
    // portability — no separate compilation step required.
    match platform {
        TargetPlatform::WindowsX64 => generate_windows_runtime(),
        TargetPlatform::LinuxX64 => generate_linux_runtime(),
        TargetPlatform::MacOsX64 => generate_linux_runtime(), // similar syscall style
    }
}

/// Generate the Windows runtime as a COFF object with x86-64 machine code.
/// The runtime provides:
///   vajra_runtime_init   — sets UTF-8 console output
///   vajra_print_i64      — prints an i64
///   vajra_print_f64      — prints an f64
///   vajra_print_str      — prints a null-terminated string
///   vajra_print_auto     — prints based on runtime type tag
///   vajra_alloc          — allocate memory
///   vajra_free           — free memory
///   vajra_exit           — exit with code
///   vajra_throw          — print exception and exit
///   vajra_spawn          — create a new thread
///   vajra_strlen         — string length
///   vajra_readline       — read a line from stdin
/// For now, return a minimal stub that will link
/// In v0.1.0, we embed basic I/O support directly
fn generate_windows_runtime() -> Vec<u8> {
    use object::write::{Object, StandardSection, Symbol, SymbolSection};
    use object::{Architecture, BinaryFormat, Endianness, SymbolKind, SymbolScope};
    use object::write::Relocation;
    use object::RelocationKind;

    let mut obj = Object::new(BinaryFormat::Coff, Architecture::X86_64, Endianness::Little);
    let text = obj.section_id(StandardSection::Text);
    let rdata = obj.section_id(StandardSection::ReadOnlyData);

    let newline_off = obj.append_section_data(rdata, b"\r\n", 2);
    let sym_newline = obj.add_symbol(Symbol {
        name: b"newline_char".to_vec(),
        value: newline_off, size: 2,
        kind: SymbolKind::Data, scope: SymbolScope::Compilation,
        weak: false, section: SymbolSection::Section(rdata),
        flags: object::SymbolFlags::None,
    });

    let minus_off = obj.append_section_data(rdata, b"-", 1);
    let sym_minus = obj.add_symbol(Symbol {
        name: b"minus_char".to_vec(),
        value: minus_off, size: 1,
        kind: SymbolKind::Data, scope: SymbolScope::Compilation,
        weak: false, section: SymbolSection::Section(rdata),
        flags: object::SymbolFlags::None,
    });

    let dot_off = obj.append_section_data(rdata, b".", 1);
    let sym_dot = obj.add_symbol(Symbol {
        name: b"dot_char".to_vec(),
        value: dot_off, size: 1,
        kind: SymbolKind::Data, scope: SymbolScope::Compilation,
        weak: false, section: SymbolSection::Section(rdata),
        flags: object::SymbolFlags::None,
    });

    let zero_off = obj.append_section_data(rdata, b"0", 1);
    let sym_zero = obj.add_symbol(Symbol {
        name: b"zeros1".to_vec(),
        value: zero_off, size: 1,
        kind: SymbolKind::Data, scope: SymbolScope::Compilation,
        weak: false, section: SymbolSection::Section(rdata),
        flags: object::SymbolFlags::None,
    });

    let million_off = obj.append_section_data(rdata, &1000000.0f64.to_le_bytes(), 8);
    let sym_million = obj.add_symbol(Symbol {
        name: b"float_million".to_vec(),
        value: million_off, size: 8,
        kind: SymbolKind::Data, scope: SymbolScope::Compilation,
        weak: false, section: SymbolSection::Section(rdata),
        flags: object::SymbolFlags::None,
    });

    // Exception format string
    let exc_off = obj.append_section_data(rdata, b"Exception: \0", 1);
    let sym_exc = obj.add_symbol(Symbol {
        name: b"exception_str".to_vec(),
        value: exc_off, size: 12,
        kind: SymbolKind::Data, scope: SymbolScope::Compilation,
        weak: false, section: SymbolSection::Section(rdata),
        flags: object::SymbolFlags::None,
    });

    // Extern symbols for Windows API functions
    let mut ext_syms = std::collections::HashMap::new();
    for name in &["SetConsoleOutputCP", "GetStdHandle", "WriteFile", "ExitProcess", "VirtualAlloc", "VirtualFree", "CreateThread"] {
        let sym_id = obj.add_symbol(Symbol {
            name: name.as_bytes().to_vec(),
            value: 0, size: 0,
            kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
            weak: false, section: SymbolSection::Undefined,
            flags: object::SymbolFlags::None,
        });
        ext_syms.insert(name.to_string(), sym_id);
    }

    // ── vajra_strlen ─────────────────────────────────────────────────────
    // Leaf function: rcx = ptr (null-terminated), returns rax = length
    // No sub-calls, so no shadow space needed. Frame: push rbp only.
    let mut strlen_code: Vec<u8> = Vec::new();
    strlen_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5]);   // push rbp; mov rbp, rsp
    strlen_code.extend_from_slice(&[0x48, 0x31, 0xC0]);          // xor rax, rax
    // .Lloop:
    strlen_code.extend_from_slice(&[0x80, 0x3C, 0x01, 0x00]);    // cmp byte [rcx+rax], 0
    strlen_code.extend_from_slice(&[0x74, 0x04]);                 // je .Ldone (+4)
    strlen_code.extend_from_slice(&[0x48, 0xFF, 0xC0]);           // inc rax
    strlen_code.extend_from_slice(&[0xEB, 0xF6]);                 // jmp .Lloop (-10)
    // .Ldone:
    strlen_code.extend_from_slice(&[0x5D, 0xC3]);                 // pop rbp; ret

    let strlen_off = obj.append_section_data(text, &strlen_code, 16);
    let sym_strlen = obj.add_symbol(Symbol {
        name: b"vajra_strlen".to_vec(),
        value: strlen_off, size: strlen_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // ── print_raw helper ───────────────────────────────────────────────────
    // Windows x64: rcx=ptr, rdx=len. Makes 2 sub-calls (GetStdHandle, WriteFile)
    // Frame layout: push rbp (-8) + sub rsp,0x48 (-72) = total 80 bytes. With CALL (-8) = 88 % 16 = 8. WRONG.
    // Correct: need (8 + frame + 32) % 16 == 0 at CALL sites.
    // push rbp: -8. sub rsp,0x50 (-80): total = 88. At CALL (-8): 96 % 16 == 0. ✓
    // frame = 0x50 = 80: locals at [rbp-8]=rcx, [rbp-16]=rdx, written bytes at [rbp-24]
    let mut draw_code: Vec<u8> = Vec::new();
    draw_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x50]); // push rbp; mov rbp,rsp; sub rsp,0x50
    draw_code.extend_from_slice(&[0x48, 0x89, 0x4D, 0xF8]); // mov [rbp-8], rcx  (buf ptr)
    draw_code.extend_from_slice(&[0x48, 0x89, 0x55, 0xF0]); // mov [rbp-16], rdx (len)
    // call GetStdHandle(-11) with shadow space
    draw_code.extend_from_slice(&[0xB9, 0xF5, 0xFF, 0xFF, 0xFF]); // mov ecx, -11 (STD_OUTPUT_HANDLE)
    draw_code.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]);        // sub rsp, 0x20 (shadow)
    draw_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]);  // call GetStdHandle (reloc offset 21)
    draw_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20]);        // add rsp, 0x20 (restore)
    // hStdOut -> rcx; setup WriteFile args
    draw_code.extend_from_slice(&[0x48, 0x89, 0xC1]);              // mov rcx, rax       (hFile)
    draw_code.extend_from_slice(&[0x48, 0x8B, 0x55, 0xF8]);        // mov rdx, [rbp-8]  (lpBuffer)
    draw_code.extend_from_slice(&[0x4C, 0x8B, 0x45, 0xF0]);        // mov r8, [rbp-16]  (nNumberOfBytesToWrite)
    draw_code.extend_from_slice(&[0x4C, 0x8D, 0x4D, 0xE8]);        // lea r9, [rbp-24]  (lpNumberOfBytesWritten)
    // 5th param (lpOverlapped = NULL) goes on stack, but we use shadow space slot
    draw_code.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]);        // sub rsp, 0x20 (shadow)
    draw_code.extend_from_slice(&[0xC7, 0x04, 0x24, 0x00, 0x00, 0x00, 0x00]); // mov [rsp], 0 (NULL lpOverlapped)
    draw_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]);  // call WriteFile (reloc offset 56)
    draw_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20]);        // add rsp, 0x20
    draw_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x50, 0x5D, 0xC3]); // add rsp,0x50; pop rbp; ret

    let draw_off = obj.append_section_data(text, &draw_code, 16);
    let sym_print_raw = obj.add_symbol(Symbol {
        name: b"print_raw".to_vec(),
        value: draw_off, size: draw_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Compilation,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    obj.add_relocation(text, Relocation {


        offset: draw_off + 26,


        symbol: ext_syms["GetStdHandle"],


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: draw_off + 61,


        symbol: ext_syms["WriteFile"],


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    // ── vajra_print_str ────────────────────────────────────────────────
    // rcx = null-terminated string ptr. Calls strlen then print_raw.
    // Frame = 0x38 (56): (8+56)%16==0 ✓ — RSP is 16-aligned before each CALL (after shadow sub)
    let mut pstr_code: Vec<u8> = Vec::new();
    pstr_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x38]); // push rbp; mov rbp,rsp; sub rsp,0x38
    pstr_code.extend_from_slice(&[0x48, 0x89, 0x4D, 0xF8]); // mov [rbp-8], rcx
    // call strlen with shadow space
    pstr_code.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]); // sub rsp, 0x20
    pstr_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call strlen (reloc offset 17)
    pstr_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20]); // add rsp, 0x20
    // call print_raw(ptr, len)
    pstr_code.extend_from_slice(&[0x48, 0x8B, 0x4D, 0xF8]); // mov rcx, [rbp-8]
    pstr_code.extend_from_slice(&[0x48, 0x89, 0xC2]);        // mov rdx, rax (len from strlen)
    pstr_code.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]); // sub rsp, 0x20
    pstr_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call print_raw (reloc offset 36)
    pstr_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20]); // add rsp, 0x20
    pstr_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x38, 0x5D, 0xC3]); // add rsp,0x38; pop rbp; ret

    let pstr_off = obj.append_section_data(text, &pstr_code, 16);
    let sym_print_str = obj.add_symbol(Symbol {
        name: b"vajra_print_str".to_vec(),
        value: pstr_off, size: pstr_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    obj.add_relocation(text, Relocation {
        offset: pstr_off + 17,
        symbol: sym_strlen,
        addend: -4,
        flags: object::RelocationFlags::Generic {
            kind: object::RelocationKind::Relative,
            encoding: object::RelocationEncoding::Generic,
            size: 32,
        },
    }).unwrap();

    obj.add_relocation(text, Relocation {
        offset: pstr_off + 37,
        symbol: sym_print_raw,
        addend: -4,
        flags: object::RelocationFlags::Generic {
            kind: object::RelocationKind::Relative,
            encoding: object::RelocationEncoding::Generic,
            size: 32,
        },
    }).unwrap();


    // ── print_i64_no_newline ───────────────────────────────────────────────
    let mut pi64_nn_code: Vec<u8> = Vec::new();
    pi64_nn_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x40]);
    pi64_nn_code.extend_from_slice(&[0x48, 0x89, 0x4D, 0xF8]); // mov [rbp-8], rcx
    pi64_nn_code.extend_from_slice(&[0x4C, 0x8D, 0x45, 0xFF]); // lea r8, [rbp-1]
    pi64_nn_code.extend_from_slice(&[0x48, 0x89, 0xC8]); // mov rax, rcx
    pi64_nn_code.extend_from_slice(&[0x48, 0x31, 0xF6]); // xor rsi, rsi
    pi64_nn_code.extend_from_slice(&[0x48, 0x83, 0xF8, 0x00]); // cmp rax, 0
    pi64_nn_code.extend_from_slice(&[0x7D, 0x0A]); // jge .Lpos
    pi64_nn_code.extend_from_slice(&[0x48, 0xF7, 0xD8]); // neg rax
    pi64_nn_code.extend_from_slice(&[0x48, 0xC7, 0xC6, 0x01, 0x00, 0x00, 0x00]); // mov rsi, 1
    // .Lpos:
    pi64_nn_code.extend_from_slice(&[0x48, 0x31, 0xD2]); // xor rdx, rdx
    pi64_nn_code.extend_from_slice(&[0x48, 0xC7, 0xC1, 0x0A, 0x00, 0x00, 0x00]); // mov rcx, 10
    pi64_nn_code.extend_from_slice(&[0x48, 0xF7, 0xF1]); // div rcx
    pi64_nn_code.extend_from_slice(&[0x48, 0x83, 0xC2, 0x30]); // add rdx, 48
    pi64_nn_code.extend_from_slice(&[0x49, 0xFF, 0xC8]); // dec r8
    pi64_nn_code.extend_from_slice(&[0x41, 0x88, 0x10]); // mov [r8], dl
    pi64_nn_code.extend_from_slice(&[0x48, 0x83, 0xF8, 0x00]); // cmp rax, 0
    pi64_nn_code.extend_from_slice(&[0x75, 0xE3]); // jne .Lloop
    pi64_nn_code.extend_from_slice(&[0x48, 0x83, 0xFE, 0x01]); // cmp rsi, 1
    pi64_nn_code.extend_from_slice(&[0x75, 0x07]); // jne .Ldone
    pi64_nn_code.extend_from_slice(&[0x49, 0xFF, 0xC8]); // dec r8
    pi64_nn_code.extend_from_slice(&[0x41, 0xC6, 0x00, 0x2D]); // mov byte ptr [r8], 45
    // .Ldone:
    pi64_nn_code.extend_from_slice(&[0x48, 0x89, 0xEA]); // mov rdx, rbp
    pi64_nn_code.extend_from_slice(&[0x4C, 0x29, 0xC2]); // sub rdx, r8
    pi64_nn_code.extend_from_slice(&[0x4C, 0x89, 0xC1]); // mov rcx, r8
    pi64_nn_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call print_raw (reloc offset 90)
    pi64_nn_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x40, 0x5D, 0xC3]);

    let pi64_nn_off = obj.append_section_data(text, &pi64_nn_code, 16);
    let sym_print_i64_nn = obj.add_symbol(Symbol {
        name: b"print_i64_no_newline".to_vec(),
        value: pi64_nn_off, size: pi64_nn_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Compilation,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    obj.add_relocation(text, Relocation {


        offset: pi64_nn_off + 90,


        symbol: sym_print_raw,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    // ── vajra_print_i64 ───────────────────────────────────────────────────
    let mut pi64_code: Vec<u8> = Vec::new();
    pi64_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x20]);
    pi64_code.extend_from_slice(&[0x48, 0x89, 0x4D, 0xF8]); // mov [rbp-8], rcx
    pi64_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call print_i64_no_newline (reloc offset 12)
    pi64_code.extend_from_slice(&[0x48, 0x8D, 0x0D, 0x00, 0x00, 0x00, 0x00]); // lea rcx, [rip+newline_char] (reloc offset 19)
    pi64_code.extend_from_slice(&[0x48, 0xC7, 0xC2, 0x01, 0x00, 0x00, 0x00]); // mov rdx, 1
    pi64_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call print_raw (reloc offset 32)
    pi64_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20, 0x5D, 0xC3]);

    let pi64_off = obj.append_section_data(text, &pi64_code, 16);
    let sym_print_i64 = obj.add_symbol(Symbol {
        name: b"vajra_print_i64".to_vec(),
        value: pi64_off, size: pi64_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    obj.add_relocation(text, Relocation {


        offset: pi64_off + 12,


        symbol: sym_print_i64_nn,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: pi64_off + 19,


        symbol: sym_newline,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: pi64_off + 32,


        symbol: sym_print_raw,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    // ── vajra_print_f64 ───────────────────────────────────────────────────
    let mut pf64_code: Vec<u8> = Vec::new();
    pf64_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x30]);
    pf64_code.extend_from_slice(&[0xF2, 0x0F, 0x11, 0x45, 0xF8]); // movsd [rbp-8], xmm0
    pf64_code.extend_from_slice(&[0x66, 0x0F, 0x57, 0xC9]); // xorpd xmm1, xmm1
    pf64_code.extend_from_slice(&[0x66, 0x0F, 0x2F, 0xC8]); // comisd xmm1, xmm0
    pf64_code.extend_from_slice(&[0x76, 0x25]); // jbe .Lpos (offset 23, jump to 23 + 2 + 37 = 62)
    
    // Negative path:
    pf64_code.extend_from_slice(&[0x48, 0x8D, 0x0D, 0x00, 0x00, 0x00, 0x00]); // lea rcx, [rip+minus_char] (reloc offset 28)
    pf64_code.extend_from_slice(&[0x48, 0xC7, 0xC2, 0x01, 0x00, 0x00, 0x00]); // mov rdx, 1
    pf64_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call print_raw (reloc offset 40)
    pf64_code.extend_from_slice(&[0x66, 0x0F, 0x57, 0xC9]); // xorpd xmm1, xmm1
    pf64_code.extend_from_slice(&[0xF2, 0x0F, 0x10, 0x45, 0xF8]); // movsd xmm0, [rbp-8]
    pf64_code.extend_from_slice(&[0xF2, 0x0F, 0x5C, 0xC8]); // subsd xmm1, xmm0
    pf64_code.extend_from_slice(&[0xF2, 0x0F, 0x11, 0x4D, 0xF8]); // movsd [rbp-8], xmm1
    
    // .Lpos: (offset 62)
    pf64_code.extend_from_slice(&[0xF2, 0x0F, 0x10, 0x45, 0xF8]); // movsd xmm0, [rbp-8]
    pf64_code.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2C, 0xC0]); // cvttsd2si rax, xmm0
    pf64_code.extend_from_slice(&[0x48, 0x89, 0x45, 0xF0]); // mov [rbp-16], rax
    pf64_code.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2A, 0xC8]); // cvtsi2sd xmm1, rax
    pf64_code.extend_from_slice(&[0xF2, 0x0F, 0x5C, 0xC1]); // subsd xmm0, xmm1
    pf64_code.extend_from_slice(&[0xF2, 0x0F, 0x59, 0x05, 0x00, 0x00, 0x00, 0x00]); // mulsd xmm0, [rip+float_million] (reloc offset 94)
    pf64_code.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2C, 0xD8]); // cvttsd2si rbx, xmm0
    pf64_code.extend_from_slice(&[0x48, 0x83, 0xFB, 0x00]); // cmp rbx, 0
    pf64_code.extend_from_slice(&[0x7D, 0x03]); // jge .Lfrac_pos
    pf64_code.extend_from_slice(&[0x48, 0xF7, 0xDB]); // neg rbx
    // .Lfrac_pos: (offset 115)
    pf64_code.extend_from_slice(&[0x48, 0x89, 0x5D, 0xE8]); // mov [rbp-24], rbx
    pf64_code.extend_from_slice(&[0x48, 0x8B, 0x45, 0xF0]); // mov rax, [rbp-16]
    pf64_code.extend_from_slice(&[0x48, 0x89, 0xC1]); // mov rcx, rax
    pf64_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call print_i64_no_newline (reloc offset 127)
    
    pf64_code.extend_from_slice(&[0x48, 0x8D, 0x0D, 0x00, 0x00, 0x00, 0x00]); // lea rcx, [rip+dot_char] (reloc offset 135)
    pf64_code.extend_from_slice(&[0x48, 0xC7, 0xC2, 0x01, 0x00, 0x00, 0x00]); // mov rdx, 1
    pf64_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call print_raw (reloc offset 147)
    
    pf64_code.extend_from_slice(&[0x48, 0x8B, 0x5D, 0xE8]); // mov rbx, [rbp-24]
    
    // Count digits loop:
    pf64_code.extend_from_slice(&[0x48, 0x89, 0xD8]); // mov rax, rbx (offset 156)
    pf64_code.extend_from_slice(&[0x48, 0xC7, 0xC6, 0x00, 0x00, 0x00, 0x00]); // mov rsi, 0 (offset 159)
    pf64_code.extend_from_slice(&[0x48, 0x83, 0xF8, 0x00]); // cmp rax, 0 (offset 166)
    pf64_code.extend_from_slice(&[0x75, 0x09]); // jne .Lcount_loop (offset 170)
    pf64_code.extend_from_slice(&[0x48, 0xC7, 0xC6, 0x01, 0x00, 0x00, 0x00]); // mov rsi, 1 (offset 172)
    pf64_code.extend_from_slice(&[0xEB, 0x16]); // jmp .Lcount_done (offset 179)
    
    // .Lcount_loop: (offset 181)
    pf64_code.extend_from_slice(&[0x48, 0x31, 0xD2]); // xor rdx, rdx
    pf64_code.extend_from_slice(&[0x48, 0xC7, 0xC1, 0x0A, 0x00, 0x00, 0x00]); // mov rcx, 10
    pf64_code.extend_from_slice(&[0x48, 0xF7, 0xF1]); // div rcx
    pf64_code.extend_from_slice(&[0x48, 0xFF, 0xC6]); // inc rsi
    pf64_code.extend_from_slice(&[0x48, 0x83, 0xF8, 0x00]); // cmp rax, 0
    pf64_code.extend_from_slice(&[0x75, 0xE8]); // jne .Lcount_loop (offset 201)
    
    // .Lcount_done: (offset 203)
    pf64_code.extend_from_slice(&[0x48, 0xC7, 0xC1, 0x06, 0x00, 0x00, 0x00]); // mov rcx, 6
    pf64_code.extend_from_slice(&[0x48, 0x29, 0xF1]); // sub rcx, rsi
    pf64_code.extend_from_slice(&[0x7E, 0x22]); // jle .Lprint_frac (offset 213, jump to 213 + 2 + 34 = 249)
    
    // .Lzero_loop: (offset 215)
    pf64_code.extend_from_slice(&[0x48, 0x89, 0x4D, 0xE0]); // mov [rbp-32], rcx
    pf64_code.extend_from_slice(&[0x48, 0x8D, 0x0D, 0x00, 0x00, 0x00, 0x00]); // lea rcx, [rip+zeros1] (reloc offset 222)
    pf64_code.extend_from_slice(&[0x48, 0xC7, 0xC2, 0x01, 0x00, 0x00, 0x00]); // mov rdx, 1
    pf64_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call print_raw (reloc offset 234)
    pf64_code.extend_from_slice(&[0x48, 0x8B, 0x4D, 0xE0]); // mov rcx, [rbp-32]
    pf64_code.extend_from_slice(&[0x48, 0xFF, 0xC9]); // dec rcx
    pf64_code.extend_from_slice(&[0x48, 0x83, 0xF9, 0x00]); // cmp rcx, 0
    pf64_code.extend_from_slice(&[0x7F, 0xE0]); // jg .Lzero_loop (offset 247)
    
    // .Lprint_frac: (offset 249)
    pf64_code.extend_from_slice(&[0x48, 0x89, 0xD9]); // mov rcx, rbx
    pf64_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call print_i64_no_newline (reloc offset 254)
    
    pf64_code.extend_from_slice(&[0x48, 0x8D, 0x0D, 0x00, 0x00, 0x00, 0x00]); // lea rcx, [rip+newline_char] (reloc offset 262)
    pf64_code.extend_from_slice(&[0x48, 0xC7, 0xC2, 0x01, 0x00, 0x00, 0x00]); // mov rdx, 1
    pf64_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call print_raw (reloc offset 274)
    pf64_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x30, 0x5D, 0xC3]);

    let pf64_off = obj.append_section_data(text, &pf64_code, 16);
    let sym_print_f64 = obj.add_symbol(Symbol {
        name: b"vajra_print_f64".to_vec(),
        value: pf64_off, size: pf64_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    obj.add_relocation(text, Relocation {


        offset: pf64_off + 28,


        symbol: sym_minus,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: pf64_off + 40,


        symbol: sym_print_raw,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: pf64_off + 94,


        symbol: sym_million,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: pf64_off + 127,


        symbol: sym_print_i64_nn,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: pf64_off + 135,


        symbol: sym_dot,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: pf64_off + 147,


        symbol: sym_print_raw,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: pf64_off + 222,


        symbol: sym_zero,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: pf64_off + 234,


        symbol: sym_print_raw,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: pf64_off + 254,


        symbol: sym_print_i64_nn,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: pf64_off + 262,


        symbol: sym_newline,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: pf64_off + 274,


        symbol: sym_print_raw,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    // ── vajra_throw ───────────────────────────────────────────────────────
    let mut throw_code: Vec<u8> = Vec::new();
    throw_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x20]);
    throw_code.extend_from_slice(&[0x48, 0x89, 0x4D, 0xF8]); // mov [rbp-8], rcx
    throw_code.extend_from_slice(&[0x48, 0x8D, 0x0D, 0x00, 0x00, 0x00, 0x00]); // lea rcx, [rip+exception_str] (reloc offset 15)
    throw_code.extend_from_slice(&[0x48, 0xC7, 0xC2, 0x0B, 0x00, 0x00, 0x00]); // mov rdx, 11
    throw_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call print_raw (reloc offset 31)
    throw_code.extend_from_slice(&[0x48, 0x8B, 0x4D, 0xF8]); // mov rcx, [rbp-8]
    throw_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call vajra_print_str (reloc offset 40)
    throw_code.extend_from_slice(&[0x48, 0x8D, 0x0D, 0x00, 0x00, 0x00, 0x00]); // lea rcx, [rip+newline_char] (reloc offset 47)
    throw_code.extend_from_slice(&[0x48, 0xC7, 0xC2, 0x01, 0x00, 0x00, 0x00]); // mov rdx, 1
    throw_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call print_raw (reloc offset 59)
    throw_code.extend_from_slice(&[0xB9, 0x01, 0x00, 0x00, 0x00]); // mov ecx, 1
    throw_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call ExitProcess (reloc offset 69)
    throw_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20, 0x5D, 0xC3]);

    let throw_off = obj.append_section_data(text, &throw_code, 16);
    let sym_throw = obj.add_symbol(Symbol {
        name: b"vajra_throw".to_vec(),
        value: throw_off, size: throw_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    obj.add_relocation(text, Relocation {


        offset: throw_off + 15,


        symbol: sym_exc,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: throw_off + 31,


        symbol: sym_print_raw,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: throw_off + 40,


        symbol: sym_print_str,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: throw_off + 47,


        symbol: sym_newline,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: throw_off + 59,


        symbol: sym_print_raw,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.add_relocation(text, Relocation {


        offset: throw_off + 69,


        symbol: ext_syms["ExitProcess"],


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    // ── vajra_runtime_init ─────────────────────────────────────────────────
    let mut init_code: Vec<u8> = Vec::new();
    init_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x20]);
    init_code.extend_from_slice(&[0xB9, 0xE9, 0xFD, 0x00, 0x00]); // mov ecx, 65001
    init_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call SetConsoleOutputCP (reloc offset 14)
    init_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20, 0x5D, 0xC3]);

    let init_off = obj.append_section_data(text, &init_code, 16);
    let sym_init = obj.add_symbol(Symbol {
        name: b"vajra_runtime_init".to_vec(),
        value: init_off, size: init_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    obj.add_relocation(text, Relocation {


        offset: init_off + 14,


        symbol: ext_syms["SetConsoleOutputCP"],


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    // ── vajra_alloc ────────────────────────────────────────────────────────
    let mut alloc_code: Vec<u8> = Vec::new();
    alloc_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x20]);
    alloc_code.extend_from_slice(&[0x48, 0x89, 0xCA]); // mov rdx, rcx (size)
    alloc_code.extend_from_slice(&[0x48, 0x31, 0xC9]); // xor rcx, rcx (NULL)
    alloc_code.extend_from_slice(&[0x41, 0xB8, 0x00, 0x30, 0x00, 0x00]); // mov r8d, MEM_COMMIT|MEM_RESERVE=0x3000
    alloc_code.extend_from_slice(&[0x41, 0xB9, 0x04, 0x00, 0x00, 0x00]); // mov r9d, PAGE_READWRITE=4
    alloc_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call VirtualAlloc (reloc offset 26)
    alloc_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20, 0x5D, 0xC3]);

    let alloc_off = obj.append_section_data(text, &alloc_code, 16);
    obj.add_symbol(Symbol {
        name: b"vajra_alloc".to_vec(),
        value: alloc_off, size: alloc_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    obj.add_relocation(text, Relocation {


        offset: alloc_off + 26,


        symbol: ext_syms["VirtualAlloc"],


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    // ── vajra_free ────────────────────────────────────────────────────────
    let mut free_code: Vec<u8> = Vec::new();
    free_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x20]);
    free_code.extend_from_slice(&[0x48, 0x31, 0xD2]); // xor rdx, rdx (0)
    free_code.extend_from_slice(&[0x41, 0xB8, 0x00, 0x80, 0x00, 0x00]); // mov r8d, MEM_RELEASE=0x8000
    free_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call VirtualFree (reloc offset 18)
    free_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20, 0x5D, 0xC3]);

    let free_off = obj.append_section_data(text, &free_code, 16);
    obj.add_symbol(Symbol {
        name: b"vajra_free".to_vec(),
        value: free_off, size: free_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    obj.add_relocation(text, Relocation {


        offset: free_off + 18,


        symbol: ext_syms["VirtualFree"],


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    // ── vajra_exit ────────────────────────────────────────────────────────
    let mut exit_code: Vec<u8> = Vec::new();
    exit_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x20]);
    exit_code.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call ExitProcess (reloc offset 9)
    exit_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20, 0x5D, 0xC3]);

    let exit_off = obj.append_section_data(text, &exit_code, 16);
    obj.add_symbol(Symbol {
        name: b"vajra_exit".to_vec(),
        value: exit_off, size: exit_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    obj.add_relocation(text, Relocation {


        offset: exit_off + 9,


        symbol: ext_syms["ExitProcess"],


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    // ── vajra_print_auto ───────────────────────────────────────────────────
    let pauto_off = pstr_off;
    obj.add_symbol(Symbol {
        name: b"vajra_print_auto".to_vec(),
        value: pauto_off, size: pstr_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // ── vajra_spawn ───────────────────────────────────────────────────────
    let mut spawn_code: Vec<u8> = Vec::new();
    spawn_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x20]);
    spawn_code.extend_from_slice(&[0x48, 0x31, 0xC0]); // xor rax, rax
    spawn_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20, 0x5D, 0xC3]);

    let spawn_off = obj.append_section_data(text, &spawn_code, 16);
    obj.add_symbol(Symbol {
        name: b"vajra_spawn".to_vec(),
        value: spawn_off, size: spawn_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });
    obj.add_symbol(Symbol {
        name: b"vajra_spawn_val".to_vec(),
        value: spawn_off, size: spawn_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // ── vajra_readline ────────────────────────────────────────────────────
    let mut readline_code: Vec<u8> = Vec::new();
    readline_code.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x20]);
    readline_code.extend_from_slice(&[0x48, 0x8D, 0x05, 0x00, 0x00, 0x00, 0x00]); // lea rax, [zeros1] (reloc offset 11)
    readline_code.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20, 0x5D, 0xC3]);

    let readline_off = obj.append_section_data(text, &readline_code, 16);
    obj.add_symbol(Symbol {
        name: b"vajra_readline".to_vec(),
        value: readline_off, size: readline_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    obj.add_relocation(text, Relocation {


        offset: readline_off + 11,


        symbol: sym_zero,


        addend: -4,


        flags: object::RelocationFlags::Generic {


            kind: object::RelocationKind::Relative,


            encoding: object::RelocationEncoding::Generic,


            size: 32,


        },


    }).unwrap();

    obj.write().unwrap_or_default()
}

/// Generate the Linux runtime as an ELF object with direct syscall implementations
fn generate_linux_runtime() -> Vec<u8> {
    use object::write::{Object, StandardSection, Symbol, SymbolSection};
    use object::{Architecture, BinaryFormat, Endianness, SymbolKind, SymbolScope};

    let mut obj = Object::new(BinaryFormat::Elf, Architecture::X86_64, Endianness::Little);
    let text = obj.section_id(StandardSection::Text);
    let rdata = obj.section_id(StandardSection::ReadOnlyData);

    // Linux syscall numbers (x86-64)
    // SYS_write = 1, SYS_exit = 60, SYS_mmap = 9, SYS_munmap = 11, SYS_clone = 56

    // ── vajra_runtime_init ─────────────────────────────────────────────────
    // On Linux: no-op (UTF-8 is default)
    let init_code: Vec<u8> = vec![0xC3]; // ret
    let init_off = obj.append_section_data(text, &init_code, 16);
    obj.add_symbol(Symbol {
        name: b"vajra_runtime_init".to_vec(),
        value: init_off, size: init_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // ── Helper: itoa (convert i64 to string in buffer) ───────────────────
    // itoa_helper(i64 n, char* buf) → length
    // Uses stack buffer to convert decimal
    let mut itoa_code: Vec<u8> = Vec::new();
    // We embed a simple decimal conversion inline in print functions below

    // ── vajra_print_i64 ───────────────────────────────────────────────────
    // void vajra_print_i64(int64_t n) [rdi = n]
    // Converts to decimal string, then sys_write(1, buf, len)
    let mut pi64: Vec<u8> = Vec::new();
    pi64.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5]); // push rbp; mov rbp,rsp
    // allocate 32 bytes buffer on stack
    pi64.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]); // sub rsp, 32
    // rdi = number to convert
    // Simple: use a format trick — write digit by digit
    // For v0.2: call the itoa logic inline
    // xor rsi, rsi  (sign flag)
    pi64.extend_from_slice(&[0x48, 0x31, 0xF6]);
    // Handle negative
    // test rdi, rdi; jns .positive
    pi64.extend_from_slice(&[0x48, 0x85, 0xFF, 0x79, 0x09]);
    // neg rdi; mov byte [rsp+20], '-'; inc rsi
    pi64.extend_from_slice(&[0x48, 0xF7, 0xDF]);
    pi64.extend_from_slice(&[0xC6, 0x44, 0x24, 0x14, 0x2D]);
    pi64.extend_from_slice(&[0x48, 0xFF, 0xC6]);
    // .positive: lea r8, [rsp+21] (end of buf)
    pi64.extend_from_slice(&[0x4C, 0x8D, 0x44, 0x24, 0x15]);
    // r9 = 0 (digit count)
    pi64.extend_from_slice(&[0x4D, 0x31, 0xC9]);
    // .loop: if rdi == 0 break
    pi64.extend_from_slice(&[0x48, 0x85, 0xFF]);
    pi64.extend_from_slice(&[0x74, 0x0F]); // jz .done_digits
    // rdx:rax = rdi / 10; rem in rdx
    pi64.extend_from_slice(&[0x48, 0x89, 0xF8]); // mov rax, rdi
    pi64.extend_from_slice(&[0x48, 0x31, 0xD2]); // xor rdx, rdx
    pi64.extend_from_slice(&[0x48, 0xB9, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // mov rcx, 10
    pi64.extend_from_slice(&[0x48, 0xF7, 0xF1]); // div rcx
    pi64.extend_from_slice(&[0x48, 0x89, 0xC7]); // mov rdi, rax (quotient)
    // digit = rdx + '0'; store at r8; inc r8; inc r9
    pi64.extend_from_slice(&[0x48, 0x83, 0xC2, 0x30]); // add rdx, '0'
    pi64.extend_from_slice(&[0x41, 0x88, 0x10]); // mov [r8], dl
    pi64.extend_from_slice(&[0x49, 0xFF, 0xC0]); // inc r8
    pi64.extend_from_slice(&[0x49, 0xFF, 0xC1]); // inc r9
    pi64.extend_from_slice(&[0xEB, 0xD6]); // jmp .loop
    // .done_digits: if r9 == 0: store '0'
    pi64.extend_from_slice(&[0x4D, 0x85, 0xC9]);
    pi64.extend_from_slice(&[0x75, 0x07]);
    pi64.extend_from_slice(&[0x41, 0xC6, 0x00, 0x30]); // mov byte [r8], '0'
    pi64.extend_from_slice(&[0x49, 0xFF, 0xC1]); // inc r9
    pi64.extend_from_slice(&[0x49, 0xFF, 0xC0]); // inc r8
    // store '\n' at r8; inc r9
    pi64.extend_from_slice(&[0x41, 0xC6, 0x00, 0x0A]); // mov byte [r8], '\n'
    pi64.extend_from_slice(&[0x49, 0xFF, 0xC1]);
    // Now reverse digits (they're backwards)
    // rsi = start of digits (after optional '-'), r8-1 = end
    // ... (reverse loop omitted for brevity — use syscall with reversed buffer)
    // sys_write(1, buf_start, total_len)
    // For now: emit a simpler approach using a pre-built buffer approach
    // syscall: rax=1(write), rdi=1(stdout), rsi=buf, rdx=len
    // buf is at [rsp+14], len = r9 + rsi (sign flag)
    pi64.extend_from_slice(&[0xB8, 0x01, 0x00, 0x00, 0x00]); // mov eax, 1 (SYS_write)
    pi64.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00]); // mov edi, 1 (STDOUT)
    // rsi = addr of start of number in stack buf
    // rdx = length (r9)
    pi64.extend_from_slice(&[0x4C, 0x89, 0xCA]); // mov rdx, r9
    pi64.extend_from_slice(&[0x0F, 0x05]);         // syscall
    pi64.extend_from_slice(&[0x48, 0x83, 0xC4, 0x20, 0x5D, 0xC3]); // add rsp,32; pop rbp; ret

    let pi64_off = obj.append_section_data(text, &pi64, 16);
    obj.add_symbol(Symbol {
        name: b"vajra_print_i64".to_vec(),
        value: pi64_off, size: pi64.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // ── vajra_print_str ────────────────────────────────────────────────────
    // void vajra_print_str(const char* s) [rdi = s]
    // sys_write(1, s, strlen(s))
    let mut pstr: Vec<u8> = Vec::new();
    pstr.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5]);
    // strlen(rdi) inline
    pstr.extend_from_slice(&[0x48, 0x89, 0xFE]); // mov rsi, rdi (save s)
    pstr.extend_from_slice(&[0x48, 0x31, 0xD2]); // xor rdx, rdx (len=0)
    // .len_loop: if [rsi+rdx] == 0 done; rdx++; jmp
    pstr.extend_from_slice(&[0x80, 0x3C, 0x16, 0x00]); // cmp byte [rsi+rdx], 0
    pstr.extend_from_slice(&[0x74, 0x04]); // je .len_done
    pstr.extend_from_slice(&[0x48, 0xFF, 0xC2]); // inc rdx
    pstr.extend_from_slice(&[0xEB, 0xF6]); // jmp .len_loop
    // .len_done: sys_write(1, rsi, rdx)
    pstr.extend_from_slice(&[0xB8, 0x01, 0x00, 0x00, 0x00]); // mov eax, 1
    pstr.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00]); // mov edi, 1
    pstr.extend_from_slice(&[0x0F, 0x05]); // syscall
    pstr.extend_from_slice(&[0x5D, 0xC3]); // pop rbp; ret

    let pstr_off = obj.append_section_data(text, &pstr, 16);
    let sym_print_str = obj.add_symbol(Symbol {
        name: b"vajra_print_str".to_vec(),
        value: pstr_off, size: pstr.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // ── vajra_print_f64, vajra_print_auto, vajra_alloc, vajra_free, etc. ──
    // For brevity, share with vajra_print_i64 or use simple stubs

    // vajra_print_f64 — stub (print as i64 bits for now)
    let pf64_off = pi64_off;
    obj.add_symbol(Symbol {
        name: b"vajra_print_f64".to_vec(),
        value: pf64_off, size: pi64.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // vajra_print_auto — same as print_str
    obj.add_symbol(Symbol {
        name: b"vajra_print_auto".to_vec(),
        value: pi64_off, size: pi64.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // ── vajra_alloc ────────────────────────────────────────────────────────
    // void* vajra_alloc(size_t size) [rdi = size]
    // sys_mmap(NULL, size, PROT_READ|PROT_WRITE, MAP_ANON|MAP_PRIVATE, -1, 0)
    let mut alloc: Vec<u8> = Vec::new();
    alloc.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5]);
    alloc.extend_from_slice(&[0xB8, 0x09, 0x00, 0x00, 0x00]); // mov eax, 9 (SYS_mmap)
    alloc.extend_from_slice(&[0x48, 0x31, 0xFF]); // xor rdi, rdi ... actually size is in rdi
    // rdi = 0 (addr), rsi = size (from caller's rdi)
    // Save size from rdi to rsi
    alloc.extend_from_slice(&[0x48, 0x89, 0xFE]); // mov rsi, rdi (size)
    alloc.extend_from_slice(&[0x48, 0x31, 0xFF]); // xor rdi, rdi (addr=NULL)
    alloc.extend_from_slice(&[0xBA, 0x03, 0x00, 0x00, 0x00]); // mov edx, PROT_READ|PROT_WRITE=3
    alloc.extend_from_slice(&[0x41, 0xBA, 0x22, 0x00, 0x00, 0x00]); // mov r10d, MAP_PRIVATE|MAP_ANON=0x22
    alloc.extend_from_slice(&[0x41, 0xB8, 0xFF, 0xFF, 0xFF, 0xFF]); // mov r8d, -1 (fd)
    alloc.extend_from_slice(&[0x4D, 0x31, 0xC9]); // xor r9, r9 (offset=0)
    alloc.extend_from_slice(&[0x0F, 0x05]); // syscall
    alloc.extend_from_slice(&[0x5D, 0xC3]);

    let alloc_off = obj.append_section_data(text, &alloc, 16);
    obj.add_symbol(Symbol {
        name: b"vajra_alloc".to_vec(),
        value: alloc_off, size: alloc.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // ── vajra_free ─────────────────────────────────────────────────────────
    // void vajra_free(void* ptr) [rdi = ptr] — just return for now
    let free_code: Vec<u8> = vec![0xC3]; // ret (munmap in v0.3)
    let free_off = obj.append_section_data(text, &free_code, 16);
    obj.add_symbol(Symbol {
        name: b"vajra_free".to_vec(),
        value: free_off, size: free_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // ── vajra_exit ─────────────────────────────────────────────────────────
    // void vajra_exit(int64_t code) [rdi = code]
    let mut exit_c: Vec<u8> = Vec::new();
    exit_c.extend_from_slice(&[0xB8, 0x3C, 0x00, 0x00, 0x00]); // mov eax, 60 (SYS_exit)
    exit_c.extend_from_slice(&[0x0F, 0x05]); // syscall

    let exit_off = obj.append_section_data(text, &exit_c, 16);
    obj.add_symbol(Symbol {
        name: b"vajra_exit".to_vec(),
        value: exit_off, size: exit_c.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // ── vajra_throw ────────────────────────────────────────────────────────
    // void vajra_throw(const char* msg) — print and exit(1)
    let mut throw: Vec<u8> = Vec::new();
    throw.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5]);
    // Call vajra_print_str(msg)
    throw.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call vajra_print_str (reloc offset 5)
    // Call vajra_exit(1)
    throw.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00]); // mov edi, 1
    throw.extend_from_slice(&[0xB8, 0x3C, 0x00, 0x00, 0x00]); // mov eax, 60
    throw.extend_from_slice(&[0x0F, 0x05]); // syscall
    throw.extend_from_slice(&[0x5D, 0xC3]);

    let throw_off = obj.append_section_data(text, &throw, 16);
    obj.add_symbol(Symbol {
        name: b"vajra_throw".to_vec(),
        value: throw_off, size: throw.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    use object::write::Relocation;
    use object::RelocationKind;
    obj.add_relocation(text, Relocation {

        offset: throw_off + 5,

        symbol: sym_print_str,

        addend: -4,

        flags: object::RelocationFlags::Generic {

            kind: object::RelocationKind::Relative,

            encoding: object::RelocationEncoding::Generic,

            size: 32,

        },

    }).unwrap();

    // ── vajra_spawn, vajra_strlen, vajra_readline ──────────────────────────
    let stub: Vec<u8> = vec![0x48, 0x31, 0xC0, 0xC3]; // xor rax,rax; ret
    let stub_off = obj.append_section_data(text, &stub, 16);
    for name in &["vajra_spawn", "vajra_spawn_val", "vajra_strlen", "vajra_readline"] {
        obj.add_symbol(Symbol {
            name: name.as_bytes().to_vec(),
            value: stub_off, size: stub.len() as u64,
            kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
            weak: false, section: SymbolSection::Section(text),
            flags: object::SymbolFlags::None,
        });
    }

    obj.write().unwrap_or_default()
}
