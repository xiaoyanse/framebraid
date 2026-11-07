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
    /// 文本塔档案（A1，§4.1 开放词汇）：单 ids 输入 [batch, seq_len]（batch 动态）→
    /// 单嵌入输出 [batch, embed_dim]；仅接受 pad 即可正确编码的模型（无 mask 输入）。
    #[serde(rename = "text-encoder")]
    TextEncoder,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TensorSpec {
    pub name: String,
    #[serde(deserialize_with = "shape")]
    pub shape: Vec<usize>,
}

fn shape<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Vec<usize>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Dim {
        Fixed(usize),
        Dynamic(String),
    }
    Vec::<Dim>::deserialize(d).map(|dims| {
        dims.into_iter()
            .map(|dim| match dim {
                Dim::Fixed(n) => n,
                Dim::Dynamic(name) => {
                    let _ = name;
                    0
                }
            })
            .collect()
    })
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    #[default]
    Transposed,
    RowMajor,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoordForm {
    #[default]
    Cxcywh,
    Xyxy,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Postprocess {
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub coord_form: CoordForm,
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub id: String,
    pub task: Task,
    pub sample_sha256: String,
    pub input: TensorSpec,
    pub inputs: Vec<TensorSpec>,
    pub outputs: Vec<TensorSpec>,
    pub classes: Option<usize>,
    pub postprocess: Postprocess,
}

impl<'de> Deserialize<'de> for Profile {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            id: String,
            task: Task,
            sample_sha256: String,
            input: Option<TensorSpec>,
            inputs: Option<Vec<TensorSpec>>,
            outputs: Vec<TensorSpec>,
            classes: Option<usize>,
            #[serde(default)]
            postprocess: Postprocess,
        }
        let w = Wire::deserialize(deserializer)?;
        let inputs = w.inputs.unwrap_or_else(|| w.input.into_iter().collect());
        let input = inputs
            .first()
            .cloned()
            .ok_or_else(|| serde::de::Error::custom("missing inputs"))?;
        Ok(Self {
            id: w.id,
            task: w.task,
            sample_sha256: w.sample_sha256,
            input,
            inputs,
            outputs: w.outputs,
            classes: w.classes,
            postprocess: w.postprocess,
        })
    }
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
        .get_or_init(|| parse(include_str!("../../resources/profiles.json")))
        .as_ref()
        .map(|registry| registry.profiles.as_slice())
        .map_err(|error| Error(error.clone()))
}

fn parse(json: &str) -> std::result::Result<Registry, String> {
    let registry: Registry =
        serde_json::from_str(json).map_err(|error| format!("vision.profileUnsupported|{error}"))?;
    for profile in &registry.profiles {
        if profile.task == Task::Detect {
            if let Some(classes) = profile.classes {
                let shape = profile
                    .outputs
                    .first()
                    .map(|o| o.shape.as_slice())
                    .unwrap_or(&[]);
                let dims = shape
                    .len()
                    .checked_sub(2)
                    .map(|n| &shape[n..])
                    .unwrap_or(&[]);
                let valid = dims.len() == 2
                    && match profile.postprocess.layout {
                        Layout::Transposed => dims[0] == 4 + classes && dims[1] > 0,
                        Layout::RowMajor => dims[1] == 4 + classes && dims[0] > 0,
                    };
                if !valid {
                    return Err(format!(
                        "vision.profileMismatch|shape={shape:?},layout={:?},classes={classes}",
                        profile.postprocess.layout
                    ));
                }
            }
        }
        if profile.task == Task::TextEncoder {
            // A1 档案约束：单输入 2 维（batch 动态=0）× 单输出 2 维；多输入/输出、
            // 3 维起（含 mask 类第二输入）不在文本塔档案范围。seq 维批 D 起兼容
            // 放宽：允许动态（=0，如 text.onnx 双动态 ids）——固定声明仍精确校验。
            let input = &profile.input.shape;
            let output = profile.outputs.first().map(|o| o.shape.as_slice());
            let valid = profile.inputs.len() == 1
                && profile.outputs.len() == 1
                && input.len() == 2
                && input[0] == 0
                && output.is_some_and(|o| o.len() == 2 && o[0] == 0 && o[1] > 0);
            if !valid {
                return Err(format!(
                    "vision.profileMismatch|task=text-encoder,input={input:?},outputs={:?}",
                    profile
                        .outputs
                        .iter()
                        .map(|o| o.shape.as_slice())
                        .collect::<Vec<_>>()
                ));
            }
        }
    }
    Ok(registry)
}

pub fn find(id: &str) -> Result<&'static Profile> {
    all()?
        .iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| Error("vision.profileUnsupported".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_encoder_profile_shape_constraints() {
        let good = r#"{"profiles":[{"id":"t","task":"text-encoder","sampleSha256":"ignore","input":{"name":"input_ids","shape":["batch",77]},"outputs":[{"name":"text_embeddings","shape":["batch",512]}]}]}"#;
        assert!(parse(good).is_ok());
        // 多输入（含 mask 类第二输入）不在文本塔档案范围
        let masked = r#"{"profiles":[{"id":"t","task":"text-encoder","sampleSha256":"ignore","inputs":[{"name":"input_ids","shape":["batch",77]},{"name":"attention_mask","shape":["batch",77]}],"outputs":[{"name":"text_embeddings","shape":["batch",512]}]}]}"#;
        let error = parse(masked).err().unwrap();
        assert!(error.contains("task=text-encoder"), "{error}");
        // 3 维输出（序列级输出而非池化嵌入）拒绝
        let seq_out = r#"{"profiles":[{"id":"t","task":"text-encoder","sampleSha256":"ignore","input":{"name":"input_ids","shape":["batch",77]},"outputs":[{"name":"last_hidden","shape":["batch",77,512]}]}]}"#;
        assert!(parse(seq_out).err().is_some());
        // 固定 batch 维（无动态）拒绝——词表条目数运行期决定
        let fixed = r#"{"profiles":[{"id":"t","task":"text-encoder","sampleSha256":"ignore","input":{"name":"input_ids","shape":[1,77]},"outputs":[{"name":"text_embeddings","shape":["batch",512]}]}]}"#;
        assert!(parse(fixed).err().is_some());
        // 动态 seq（批 D 兼容放宽）：双动态 ids 档案合法
        let dynseq = r#"{"profiles":[{"id":"t","task":"text-encoder","sampleSha256":"ignore","input":{"name":"input_ids","shape":["batch","seq"]},"outputs":[{"name":"text_embeddings","shape":["batch",512]}]}]}"#;
        assert!(parse(dynseq).is_ok());
    }
    #[test]
    fn registry_carries_batch_world_and_dynseq_text_profiles() {
        // 批 D 兼容新增条目实装断言（装载面：嵌入注册表真实解析通过且形状如声明）。
        let all = all().unwrap();
        let world = all
            .iter()
            .find(|p| p.id == "detect-yolo-world-batch-v1")
            .expect("batch world entry");
        assert_eq!(world.inputs[0].shape, vec![0, 3, 640, 640]);
        assert_eq!(world.inputs[1].shape, vec![0, 0, 512]);
        assert_eq!(world.outputs[0].shape, vec![0, 0, 8400]);
        let tower = all
            .iter()
            .find(|p| p.id == "text-encoder-clip-dynseq-v1")
            .expect("dynseq text tower entry");
        assert_eq!(tower.input.shape, vec![0, 0]);
        assert_eq!(tower.outputs[0].shape, vec![0, 512]);
        // 既有条目语义不动：world-head 2-D 文本与固定 77 文本塔仍在表。
        assert!(all.iter().any(|p| p.id == "detect-yolo-world-head-v1"));
        assert!(all.iter().any(|p| p.id == "text-encoder-clip-embed-v1"));
    }
    #[test]
    fn inputs_precede_legacy_input_and_bad_head_shape_is_rejected() {
        let json = r#"{"profiles":[{"id":"x","task":"detect","sampleSha256":"ignore","input":{"name":"old","shape":[1]},"inputs":[{"name":"images","shape":[1,3,640,640]},{"name":"text","shape":["n",512]}],"outputs":[{"name":"o","shape":[1,9,7]}],"classes":4}]}"#;
        let error = parse(json).err().unwrap();
        assert!(error.contains("vision.profileMismatch|shape="));
        let good = json.replace("[1,9,7]", "[1,8,7]");
        let registry = parse(&good).unwrap();
        assert_eq!(registry.profiles[0].inputs.len(), 2);
        assert_eq!(registry.profiles[0].input.name, "images");
    }
}
