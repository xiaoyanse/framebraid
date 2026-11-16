//! D3D11 纹理注册缓存（A 线三靶点优化）：同纹理跨调用复用 cudaGraphics
//! 注册，消除每帧 register/unregister。生命周期靠 COM AddRef 保活——宿主
//! 提前 Release 不销毁纹理对象，unregister 时对象必然有效；指针键唯一性
//! 同样由引用计数保证（保活期间同地址不会复用为新纹理）。容量超限按 LRU
//! 逐出，逐出即 unregister+Release（发生在已设置 CUDA current context 的
//! 管线线程，与注册同线程惯例）。F03 起缓存为 GpuPipeline 设备级成员
//!（注册资源绑定创建时上下文，多卡各持独立缓存；进程清理由
//! pipeline::release_for_shutdown 逐设备执行）。
use super::cuda_sys::CudaApi;
use super::interop::RegisteredTexture;
use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

/// LRU 键序（纯逻辑独立可测）：touch 移尾、push 追加、超限从最旧逐出。
struct LruOrder {
    order: VecDeque<usize>,
    limit: usize,
}

impl LruOrder {
    fn new(limit: usize) -> Self {
        Self {
            order: VecDeque::new(),
            limit,
        }
    }
    fn touch(&mut self, key: usize) {
        if let Some(i) = self.order.iter().position(|k| *k == key) {
            self.order.remove(i);
            self.order.push_back(key);
        }
    }
    fn push(&mut self, key: usize) {
        self.order.push_back(key);
    }
    /// 弹出超限待逐出的最旧键；未超限返回 None。
    fn pop_overflow(&mut self, live: usize) -> Option<usize> {
        if live > self.limit && !self.order.is_empty() {
            self.order.pop_front()
        } else {
            None
        }
    }
    fn clear(&mut self) {
        self.order.clear();
    }
}

/// 缓存条目：注册纹理（COM 保活）+ 同纹理 map/unmap 串行化锁（跨线程
/// 并发同纹理时保证 map 与取 array 的原子性，生产场景一纹理一路本不并发，
/// 锁仅在异常形态下兜底正确性）。
pub struct CacheEntry {
    pub texture: RegisteredTexture,
    pub map_lock: Mutex<()>,
}

// D3D11 资源对象 free-threaded，CUDA runtime 线程安全；map/unmap 由
// map_lock 串行，unregister 仅在 Arc 归零（无借用）后发生。
unsafe impl Send for CacheEntry {}
unsafe impl Sync for CacheEntry {}

/// 指纹上限：R1.3 多路形态单 device 最多 16 张纹理；超限逐出最旧
/// （每张 1080p NV12 数组纹理 ~12.6MB，上限即最坏保活显存边界）。
const MAX_ENTRIES: usize = 16;

pub struct TextureRegistry {
    entries: HashMap<usize, Arc<CacheEntry>>,
    order: LruOrder,
}

// 裸指针仅在持锁时触碰；register/unregister 均为线程安全 runtime 调用。
unsafe impl Send for TextureRegistry {}

impl TextureRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: LruOrder::new(MAX_ENTRIES),
        }
    }

    /// 取（或注册并缓存）纹理注册条目；命中刷新 LRU，注册成功即 AddRef
    /// 保活。超限逐出最旧条目（此刻若仍有在途借用，实际 unregister 延后
    /// 到最后一个借用者 Drop——Arc 语义）。
    ///
    /// # Safety
    /// `texture` 必须为有效 ID3D11Texture2D 且在调用后持续存活（缓存保活
    /// 使其存活到条目逐出，宿主 Release 不足以提前销毁）；同 `register`。
    pub unsafe fn acquire(
        &mut self,
        api: &Arc<CudaApi>,
        texture: *mut c_void,
    ) -> Result<Arc<CacheEntry>, crate::Error> {
        let key = texture as usize;
        if let Some(entry) = self.entries.get(&key) {
            self.order.touch(key);
            return Ok(entry.clone());
        }
        let registered = unsafe { RegisteredTexture::register_keepalive(api.clone(), texture)? };
        let entry = Arc::new(CacheEntry {
            texture: registered,
            map_lock: Mutex::new(()),
        });
        self.entries.insert(key, entry.clone());
        self.order.push(key);
        while let Some(old) = self.order.pop_overflow(self.entries.len()) {
            if let Some(evicted) = self.entries.remove(&old) {
                drop(evicted); // 最后引用时 unregister → Release（字段序）
            }
        }
        Ok(entry)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 退出清理：逐条 drop（unregister → Release，字段序保证先后）。
    pub(crate) fn drain(&mut self) {
        let entries = std::mem::take(&mut self.entries);
        self.order.clear();
        for (_, entry) in entries {
            drop(entry);
        }
    }
}

impl Default for TextureRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lru_touch_moves_key_to_newest() {
        let mut lru = LruOrder::new(3);
        lru.push(1);
        lru.push(2);
        lru.push(3);
        lru.touch(1);
        assert_eq!(lru.order, VecDeque::from([2, 3, 1]));
        lru.touch(99); // 未持有键：无变化
        assert_eq!(lru.order, VecDeque::from([2, 3, 1]));
    }

    #[test]
    fn lru_overflow_evicts_oldest_first() {
        let mut lru = LruOrder::new(2);
        lru.push(1);
        lru.push(2);
        assert_eq!(lru.pop_overflow(2), None, "未超限不逐出");
        lru.push(3);
        assert_eq!(lru.pop_overflow(3), Some(1));
        assert_eq!(lru.pop_overflow(2), None, "逐出后回到限内");
    }
}
