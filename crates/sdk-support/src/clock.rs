/// Windows两库/跨进程使用同机QPC原始时基，不减进程启动点。
pub fn monotonic_ms() -> i64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Performance::{
            QueryPerformanceCounter, QueryPerformanceFrequency,
        };
        let mut counter = 0i64;
        let mut frequency = 0i64;
        // Win32写入两个本地整数，无调用方指针参与。
        unsafe {
            QueryPerformanceCounter(&mut counter);
            QueryPerformanceFrequency(&mut frequency);
        }
        if frequency <= 0 {
            return i64::MIN;
        }
        ((counter as i128 * 1000) / frequency as i128) as i64
    }
    #[cfg(not(windows))]
    {
        i64::MIN
    }
}
