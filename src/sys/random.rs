//! Random bytes from the operating system, for salts and login challenges
//! (see `net/accounts/rng.rs`, which stretches them).

/// Fill `buf` from the system's random source. `false` if there is none (the
/// browser build), in which case `buf` is left as it was.
pub fn fill(buf: &mut [u8]) -> bool {
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    {
        use core::ffi::{c_char, c_int, c_void};
        extern "C" {
            fn open(path: *const c_char, flags: c_int, ...) -> c_int;
            fn read(fd: c_int, buf: *mut c_void, n: usize) -> isize;
            fn close(fd: c_int) -> c_int;
        }
        let path = b"/dev/urandom\0";
        let fd = unsafe { open(path.as_ptr() as *const c_char, 0) };
        if fd < 0 {
            return false;
        }
        let mut got = 0usize;
        while got < buf.len() {
            let n = unsafe { read(fd, buf[got..].as_mut_ptr() as *mut c_void, buf.len() - got) };
            if n <= 0 {
                break;
            }
            got += n as usize;
        }
        unsafe { close(fd) };
        return got == buf.len();
    }
    #[cfg(windows)]
    {
        #[link(name = "advapi32", kind = "raw-dylib")]
        extern "system" {
            // RtlGenRandom, which advapi32 exports under this name (Windows XP and later).
            fn SystemFunction036(buf: *mut u8, len: u32) -> u8;
        }
        return buf.len() <= u32::MAX as usize && unsafe { SystemFunction036(buf.as_mut_ptr(), buf.len() as u32) } != 0;
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = buf;
        false
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_system_gives_different_bytes_each_time() {
        let (mut a, mut b) = ([0u8; 32], [0u8; 32]);
        assert!(super::fill(&mut a) && super::fill(&mut b));
        assert_ne!(a, b);
        assert_ne!(a, [0u8; 32]);
    }
}
