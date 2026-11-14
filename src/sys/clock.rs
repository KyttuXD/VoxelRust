//! The wall clock (seconds since 1970, UTC) and what the accounts need from the machine:
//! the time of day for "locked until 14:05", and a file only its owner may read.

/// Seconds since 1 January 1970 (UTC). 0 where the machine has no clock
/// (the browser build, which never hosts).
pub fn unix_seconds() -> u64 {
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    {
        extern "C" {
            fn time(t: *mut core::ffi::c_long) -> core::ffi::c_long;
        }
        let t = unsafe { time(core::ptr::null_mut()) };
        return if t > 0 { t as u64 } else { 0 };
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32", kind = "raw-dylib")]
        extern "system" {
            fn GetSystemTimeAsFileTime(t: *mut u64);
        }
        let mut ticks = 0u64;
        unsafe { GetSystemTimeAsFileTime(&mut ticks) };
        // 100-nanosecond steps since 1601; 11 644 473 600 seconds before 1970.
        return (ticks / 10_000_000).saturating_sub(11_644_473_600);
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

/// "14:05" (UTC) for a time from [`unix_seconds`].
pub fn clock_text(unix: u64) -> alloc::string::String {
    let day = unix % 86_400;
    alloc::format!("{:02}:{:02}", day / 3600, (day / 60) % 60)
}

/// Make a file readable by its owner only (the accounts file holds what lets
/// someone log in). Nothing on systems without file modes.
pub fn make_private(path: &str) {
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    {
        extern "C" {
            fn chmod(path: *const core::ffi::c_char, mode: u32) -> core::ffi::c_int;
        }
        let p = super::cstr(path);
        unsafe {
            chmod(p.as_ptr() as *const core::ffi::c_char, 0o600);
        }
    }
    #[cfg(not(all(unix, not(target_arch = "wasm32"))))]
    {
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_clock_runs_and_is_written_as_hours_and_minutes() {
        assert!(super::unix_seconds() > 1_700_000_000);
        assert_eq!(super::clock_text(0), "00:00");
        assert_eq!(super::clock_text(14 * 3600 + 5 * 60 + 59), "14:05");
        assert_eq!(super::clock_text(86_400 + 3600), "01:00");
    }
}
