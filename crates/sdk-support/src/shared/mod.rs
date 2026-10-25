//! 显式启用的Windows同会话CPU共享帧；协议、内核对象及C适配分开维护。
pub mod api;
pub mod layout;
mod pool;
mod windows;
mod wire;
pub use pool::Ledger;
#[cfg(test)]
mod tests;
