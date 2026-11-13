//! Entry point. The C runtime calls `main`; everything else lives in the library.
#![no_std]
#![no_main]

use core::ffi::{c_char, c_int};

#[no_mangle]
pub extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    voxelrust::app::main(argc, argv)
}

// Windows: we bring our own start-up code instead of linking the C runtime's
// (see `windows_entry`), so a Windows build needs no SDK and can be made on
// any machine. build.rs names this as the entry point.
#[cfg(windows)]
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn mainCRTStartup() -> ! {
    voxelrust::sys::windows_entry(main)
}

// Mac OS X 10.6 and 10.7 start a program at a raw entry point (the
// LC_UNIXTHREAD load command) with argc and argv on the stack; the `main`
// convention (LC_MAIN) only arrived in 10.8. Apple's linker used to supply
// this stub from crt1.o. We carry our own so that a 10.6 build needs no
// Apple SDK: scripts/build-macos-legacy.sh links with this as the entry
// point and then rewrites the load command. Ordinary macOS builds never
// call it.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
core::arch::global_asm!(
    ".globl _voxelrust_start",
    ".p2align 4",
    "_voxelrust_start:",
    "push 0",           // end-of-frames marker for debuggers
    "mov rbp, rsp",
    "and rsp, -16",     // the ABI wants a 16-byte aligned stack at the call
    "mov rdi, [rbp + 8]",  // argc
    "lea rsi, [rbp + 16]", // argv
    "call _main",
    "mov edi, eax",
    "call _exit",
    "hlt",
);
