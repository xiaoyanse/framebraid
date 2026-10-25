//! 媒体域测试公用工具（P08 2.3 抽公共）：MediaMTX/ffmpeg 自产源保障，技术门同法。
// 整文件 test-only（mod 声明处已按 test/testkit 特性门控）；
// 随 crate 迁出后经 testkit 特性供 app-core 测试复用，use 不再单独 cfg(test)
use std::time::{Duration, Instant};

/// 轮询等待条件成立，超时返回 false。
pub fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    cond()
}

/// 端口是否已监听。
pub fn port_open(port: u16) -> bool {
    std::net::TcpStream::connect(("127.0.0.1", port)).is_ok()
}

/// PATH 探测 ffmpeg 可执行文件。
pub fn which_ffmpeg() -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        });
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// MediaMTX 探测：PATH → 技术门下载目录 %TEMP%/p08-mediamtx。
pub fn which_mediamtx() -> Option<std::path::PathBuf> {
    let name = if cfg!(windows) {
        "mediamtx.exe"
    } else {
        "mediamtx"
    };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let cached = std::env::temp_dir().join("p08-mediamtx").join(name);
    cached.is_file().then_some(cached)
}

/// MediaMTX 就绪保障：已监听返回 None（复用），否则自起并等端口就绪，失败 None。
pub fn ensure_mediamtx() -> Option<std::process::Child> {
    if port_open(8554) {
        return None;
    }
    let mediamtx = which_mediamtx()?;
    let log = std::env::temp_dir()
        .join("p08-mediamtx")
        .join("mediamtx-spawn.log");
    let log_file = std::fs::File::create(&log).ok()?;
    // 工作目录指到 yml 所在处：无配置时新版默认拒绝未配置 path（实测 400/404）
    let dir = mediamtx.parent().map(|p| p.to_path_buf())?;
    let mut child = std::process::Command::new(&mediamtx)
        .current_dir(&dir)
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    if wait_until(Duration::from_secs(10), || port_open(8554)) {
        // 端口通后补缓冲：listeners 全量就绪与推流 ANNOUNCE 竞争会回 400
        std::thread::sleep(Duration::from_millis(1000));
        Some(child)
    } else {
        let _ = child.kill();
        let _ = child.wait();
        None
    }
}
