use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{cell::UnsafeCell, sync::OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Task {
    Pose,
    Detect,
    Classify,
    Segment,
    Obb,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TensorSpec {
    pub name: String,
    pub shape: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub task: Task,
    pub sample_sha256: String,
    pub input: TensorSpec,
    pub outputs: Vec<TensorSpec>,
    pub classes: Option<usize>,
}

#[derive(Deserialize)]
struct Registry {
    profiles: Vec<Profile>,
}
struct RegistryCell(UnsafeCell<OnceLock<std::result::Result<Registry, String>>>);
// 正常访问只经过OnceLock；唯一可变访问限于所有调用排空后的DLL终止。
unsafe impl Sync for RegistryCell {}
static REGISTRY: RegistryCell = RegistryCell(UnsafeCell::new(OnceLock::new()));
/// # Safety
/// 仅供外发DLL已排空后的终止钩子；宿主静态链接运行期间不能释放返回过的静态档案。
pub unsafe fn release_for_sdk_unload() {
    let slot = unsafe { &mut *REGISTRY.0.get() };
    drop(slot.take());
}

pub fn all() -> Result<&'static [Profile]> {
    unsafe { &*REGISTRY.0.get() }
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../resources/profiles.json"))
                .map_err(|error| format!("vision.profileUnsupported|{error}"))
        })
        .as_ref()
        .map(|registry| registry.profiles.as_slice())
        .map_err(|error| Error(error.clone()))
}

pub fn find(id: &str) -> Result<&'static Profile> {
    all()?
        .iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| Error("vision.profileUnsupported".into()))
}
