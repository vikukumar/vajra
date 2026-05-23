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
pub fn get_runtime_object_bytes(platform: &TargetPlatform) -> Vec<u8> {
    match platform {
        TargetPlatform::WindowsX64 => {
            #[cfg(target_os = "windows")]
            {
                generate_host_runtime()
            }
            #[cfg(not(target_os = "windows"))]
            {
                // If cross-compiling, return the checked-in Windows runtime object bytes
                include_bytes!("runtime_win.o").to_vec()
            }
        }
        TargetPlatform::LinuxX64 => {
            #[cfg(target_os = "linux")]
            {
                generate_host_runtime()
            }
            #[cfg(not(target_os = "linux"))]
            {
                // Fall back to generating ELF on the fly if cross-compiling
                generate_linux_runtime()
            }
        }
        TargetPlatform::MacOsX64 => {
            #[cfg(target_os = "macos")]
            {
                generate_host_runtime()
            }
            #[cfg(not(target_os = "macos"))]
            {
                generate_linux_runtime()
            }
        }
    }
}

/// Retrieve the host runtime object bytes compiled from runtime.rs via build.rs
fn generate_host_runtime() -> Vec<u8> {
    include_bytes!(concat!(env!("OUT_DIR"), "/runtime.o")).to_vec()
}

/// Generate the Linux runtime as an ELF object with direct syscall implementations
fn generate_linux_runtime() -> Vec<u8> {
    use object::write::{Object, StandardSection, Symbol, SymbolSection};
    use object::{Architecture, BinaryFormat, Endianness, SymbolKind, SymbolScope};

    let mut obj = Object::new(BinaryFormat::Elf, Architecture::X86_64, Endianness::Little);
    let text = obj.section_id(StandardSection::Text);
    let _rdata = obj.section_id(StandardSection::ReadOnlyData);

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

    // ── vajra_print_i64 ───────────────────────────────────────────────────
    let mut pi64: Vec<u8> = Vec::new();
    pi64.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5]); // push rbp; mov rbp,rsp
    pi64.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]); // sub rsp, 32
    pi64.extend_from_slice(&[0x48, 0x31, 0xF6]);
    pi64.extend_from_slice(&[0x48, 0x85, 0xFF, 0x79, 0x09]);
    pi64.extend_from_slice(&[0x48, 0xF7, 0xDF]);
    pi64.extend_from_slice(&[0xC6, 0x44, 0x24, 0x14, 0x2D]);
    pi64.extend_from_slice(&[0x48, 0xFF, 0xC6]);
    pi64.extend_from_slice(&[0x4C, 0x8D, 0x44, 0x24, 0x15]);
    pi64.extend_from_slice(&[0x4D, 0x31, 0xC9]);
    pi64.extend_from_slice(&[0x48, 0x85, 0xFF]);
    pi64.extend_from_slice(&[0x74, 0x0F]); // jz .done_digits
    pi64.extend_from_slice(&[0x48, 0x89, 0xF8]); // mov rax, rdi
    pi64.extend_from_slice(&[0x48, 0x31, 0xD2]); // xor rdx, rdx
    pi64.extend_from_slice(&[0x48, 0xB9, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // mov rcx, 10
    pi64.extend_from_slice(&[0x48, 0xF7, 0xF1]); // div rcx
    pi64.extend_from_slice(&[0x48, 0x89, 0xC7]); // mov rdi, rax
    pi64.extend_from_slice(&[0x48, 0x83, 0xC2, 0x30]); // add rdx, '0'
    pi64.extend_from_slice(&[0x41, 0x88, 0x10]); // mov [r8], dl
    pi64.extend_from_slice(&[0x49, 0xFF, 0xC0]); // inc r8
    pi64.extend_from_slice(&[0x49, 0xFF, 0xC1]); // inc r9
    pi64.extend_from_slice(&[0xEB, 0xD6]); // jmp .loop
    pi64.extend_from_slice(&[0x4D, 0x85, 0xC9]);
    pi64.extend_from_slice(&[0x75, 0x07]);
    pi64.extend_from_slice(&[0x41, 0xC6, 0x00, 0x30]); // mov byte [r8], '0'
    pi64.extend_from_slice(&[0x49, 0xFF, 0xC1]); // inc r9
    pi64.extend_from_slice(&[0x49, 0xFF, 0xC0]); // inc r8
    pi64.extend_from_slice(&[0x41, 0xC6, 0x00, 0x0A]); // mov byte [r8], '\n'
    pi64.extend_from_slice(&[0x49, 0xFF, 0xC1]);
    pi64.extend_from_slice(&[0xB8, 0x01, 0x00, 0x00, 0x00]); // mov eax, 1 (SYS_write)
    pi64.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00]); // mov edi, 1 (STDOUT)
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
    let mut pstr: Vec<u8> = Vec::new();
    pstr.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5]);
    pstr.extend_from_slice(&[0x48, 0x89, 0xFE]); // mov rsi, rdi
    pstr.extend_from_slice(&[0x48, 0x31, 0xD2]); // xor rdx, rdx
    pstr.extend_from_slice(&[0x80, 0x3C, 0x16, 0x00]); // cmp byte [rsi+rdx], 0
    pstr.extend_from_slice(&[0x74, 0x04]); // je .len_done
    pstr.extend_from_slice(&[0x48, 0xFF, 0xC2]); // inc rdx
    pstr.extend_from_slice(&[0xEB, 0xF6]); // jmp .len_loop
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

    // vajra_print_f64 — stub
    let pf64_off = pi64_off;
    obj.add_symbol(Symbol {
        name: b"vajra_print_f64".to_vec(),
        value: pf64_off, size: pi64.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // vajra_print_auto
    obj.add_symbol(Symbol {
        name: b"vajra_print_auto".to_vec(),
        value: pi64_off, size: pi64.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // ── vajra_alloc ────────────────────────────────────────────────────────
    let mut alloc: Vec<u8> = Vec::new();
    alloc.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5]);
    alloc.extend_from_slice(&[0xB8, 0x09, 0x00, 0x00, 0x00]); // mov eax, 9 (SYS_mmap)
    alloc.extend_from_slice(&[0x48, 0x89, 0xFE]); // mov rsi, rdi
    alloc.extend_from_slice(&[0x48, 0x31, 0xFF]); // xor rdi, rdi
    alloc.extend_from_slice(&[0xBA, 0x03, 0x00, 0x00, 0x00]); // mov edx, PROT_READ|PROT_WRITE=3
    alloc.extend_from_slice(&[0x41, 0xBA, 0x22, 0x00, 0x00, 0x00]); // mov r10d, MAP_PRIVATE|MAP_ANON=0x22
    alloc.extend_from_slice(&[0x41, 0xB8, 0xFF, 0xFF, 0xFF, 0xFF]); // mov r8d, -1
    alloc.extend_from_slice(&[0x4D, 0x31, 0xC9]); // xor r9, r9
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
    let free_code: Vec<u8> = vec![0xC3]; // ret
    let free_off = obj.append_section_data(text, &free_code, 16);
    obj.add_symbol(Symbol {
        name: b"vajra_free".to_vec(),
        value: free_off, size: free_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic,
        weak: false, section: SymbolSection::Section(text),
        flags: object::SymbolFlags::None,
    });

    // ── vajra_exit ─────────────────────────────────────────────────────────
    let mut exit_c: Vec<u8> = Vec::new();
    exit_c.extend_from_slice(&[0xB8, 0x3C, 0x00, 0x00, 0x00]); // mov eax, 60
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
    let mut throw: Vec<u8> = Vec::new();
    throw.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5]);
    throw.extend_from_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00]); // call vajra_print_str
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
