#![no_std]
#![allow(non_camel_case_types, non_snake_case, unused_unsafe, dead_code, clippy::all)]

use core::panic::PanicInfo;
use core::sync::atomic::{AtomicI32, AtomicU64, AtomicUsize, Ordering};
use core::ffi::c_void;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[cfg(target_os = "windows")]
include!("runtime_windows.rs");

#[cfg(target_os = "linux")]
include!("runtime_linux.rs");

#[cfg(target_os = "macos")]
include!("runtime_macos.rs");

#[cfg(target_os = "android")]
include!("runtime_android.rs");

#[cfg(target_os = "ios")]
include!("runtime_ios.rs");

#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "linux"),
    not(target_os = "macos"),
    not(target_os = "android"),
    not(target_os = "ios")
))]
include!("runtime_other.rs");

include!("runtime_common.rs");
