//! 瞬时凭据，由调用方注入；不持久化，故意不实现 Debug。
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Secret {
    pub username: String,
    pub password: String,
}
