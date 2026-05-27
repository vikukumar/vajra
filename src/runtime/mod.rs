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
            include_bytes!(concat!(env!("OUT_DIR"), "/runtime_win.o")).to_vec()
        }
        TargetPlatform::LinuxX64 => {
            include_bytes!(concat!(env!("OUT_DIR"), "/runtime_linux.o")).to_vec()
        }
        TargetPlatform::MacOsX64 => {
            // macOS falls back to the direct syscall Unix runtime implementation
            include_bytes!(concat!(env!("OUT_DIR"), "/runtime_linux.o")).to_vec()
        }
    }
}
