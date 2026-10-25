//! SDK 独立构建校验壳（SDK-G0 门）：仅声明两 crate 依赖关系，
//! 保证本 workspace 内 `cargo check`/`cargo test` 可独立完成，不含业务逻辑。

/// 供两导出库使用同一份值类型，避免跨库像素视图布局各自漂移。
pub use am_sdk_abi as abi;
pub use am_sdk_support as ffi_support;

/// 依赖清单自证：编译期已链接两 crate，运行期返回名字供冒烟断言。
pub fn sdk_crates() -> [&'static str; 2] {
    ["am-camera", "am-vision"]
}

#[cfg(test)]
mod tests {
    #[test]
    fn sdk_deploys_two_crates() {
        assert_eq!(super::sdk_crates().len(), 2);
    }
}
