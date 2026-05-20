/// Vajra Own Linker — Module Root
/// Links one or more object files (COFF/ELF) + embedded runtime into a native executable.
/// No MSVC link.exe, no GNU ld, no lld — 100% pure Rust.

pub mod pe;
pub mod elf;
pub mod symbol_table;

use anyhow::Result;

/// Target platform for the linked executable
#[derive(Debug, Clone, PartialEq)]
pub enum TargetPlatform {
    WindowsX64,   // PE32+ EXE
    LinuxX64,     // ELF64
    MacOsX64,     // Mach-O (planned v0.3)
}

impl TargetPlatform {
    pub fn host() -> Self {
        #[cfg(target_os = "windows")]
        return TargetPlatform::WindowsX64;
        #[cfg(target_os = "linux")]
        return TargetPlatform::LinuxX64;
        #[cfg(target_os = "macos")]
        return TargetPlatform::MacOsX64;
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        return TargetPlatform::LinuxX64;
    }

    pub fn from_triple(triple: &str) -> Self {
        if triple.contains("windows") {
            TargetPlatform::WindowsX64
        } else if triple.contains("darwin") || triple.contains("macos") {
            TargetPlatform::MacOsX64
        } else {
            TargetPlatform::LinuxX64
        }
    }

    pub fn exe_extension(&self) -> &'static str {
        match self {
            TargetPlatform::WindowsX64 => "exe",
            TargetPlatform::LinuxX64 | TargetPlatform::MacOsX64 => "",
        }
    }
}

/// Link object file bytes + embedded runtime into a complete native executable.
/// Returns the raw bytes of the executable.
pub fn link(
    obj_bytes: &[u8],
    runtime_bytes: &[u8],
    platform: &TargetPlatform,
    entry_point: &str,
) -> Result<Vec<u8>> {
    match platform {
        TargetPlatform::WindowsX64 => {
            pe::link(obj_bytes, runtime_bytes, entry_point)
        }
        TargetPlatform::LinuxX64 => {
            elf::link(obj_bytes, runtime_bytes, entry_point)
        }
        TargetPlatform::MacOsX64 => {
            anyhow::bail!("macOS Mach-O linker not yet implemented (planned for v0.1). Use --target x86_64-unknown-linux-gnu for now.")
        }
    }
}

/// High-level link function: reads obj file, links with embedded runtime, writes EXE
pub fn link_to_file(
    obj_path: &str,
    out_path: &str,
    platform: &TargetPlatform,
) -> Result<()> {
    use std::fs;
    let obj_bytes = fs::read(obj_path)
        .map_err(|e| anyhow::anyhow!("Failed to read object file '{}': {}", obj_path, e))?;

    // Get the embedded runtime bytes (compiled into vajrac itself)
    let runtime_bytes = crate::runtime::get_runtime_object_bytes(platform);

    let exe_bytes = link(&obj_bytes, &runtime_bytes, platform, "main")?;

    fs::write(out_path, &exe_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to write executable '{}': {}", out_path, e))?;

    // On Unix, make it executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(out_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(out_path, perms)?;
    }

    Ok(())
}
