// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
//! 两个导出库共用的C ABI值类型；不含业务、分配器或第三方运行库。
#![allow(non_camel_case_types)]
use core::ffi::{c_char, c_void};
mod base;
pub use base::*;
mod fields_0;

mod fields_1;

mod fields_2;

mod fields_3;

mod fields_4;

mod fields_5;

mod fields_6;

mod fields_7;

mod fields_8;

mod types_0;
pub use types_0::*;
mod types_1;
pub use types_1::*;
mod types_2;
pub use types_2::*;
mod types_3;
pub use types_3::*;
mod types_4;
pub use types_4::*;
mod types_5;
pub use types_5::*;
mod types_6;
pub use types_6::*;
mod types_7;
pub use types_7::*;
mod types_8;
pub use types_8::*;
mod defaults;
pub use defaults::*;
mod constants;
pub use constants::*;
mod status;
pub use status::*;
#[cfg(all(test, target_os = "windows", target_arch = "x86_64"))]
mod layout_tests;
