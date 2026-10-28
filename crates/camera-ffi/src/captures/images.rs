use super::*;
use crate::{
    frames::{Charge, ControlCharge},
    sources::SharedSource,
};
use am_sdk_support::buffers::Buffer;

pub struct Image {
    pub raw: Arc<io::Raw>,
    rgba: Option<Vec<u8>>,
    pub view: Box<am_frame_view_t>,
    _charge: Option<Charge>,
}
unsafe impl Send for Image {}
unsafe impl Sync for Image {}
impl Image {
    pub fn build(raw: Arc<io::Raw>, kind: u32, camera: &Arc<CameraState>) -> Result<Arc<Self>> {
        if kind == AM_CAP_ENCODED_JPEG && !raw.encoded.starts_with(&[0xff, 0xd8]) {
            return Err(AM_ERR_CAMERA_CODEC_UNSUPPORTED);
        }
        let (rgba, charge, width, height) = if kind == AM_CAP_CPU_FRAME {
            let bytes = raw.width as u64 * raw.height as u64 * 4;
            let charge = camera.charge(bytes, false)?;
            // RGB解码(3B/px)与RGBA转换(4B/px)短暂共存的峰值精确预留；EXIF
            // 旋转复用同缓冲。4K 帧峰值 ≈ 4+7 B/px + encoded ≈ 93MiB。
            let scratch = camera.charge(raw.width as u64 * raw.height as u64 * 7, false)?;
            // 解码器拥有输入Vec；这份临时复制亦先计费，峰值不会漏算。
            let copy = camera.charge(raw.encoded.len() as u64, false)?;
            let image =
                am_camera_core::media::sdk_io::decode_image(raw.encoded.clone(), 16_777_216, true)
                    .map_err(io::error)?;
            drop(copy);
            drop(scratch);
            (
                Some(image.rgba.ok_or(AM_ERR_CAMERA_CAPTURE_FAILED)?),
                Some(charge),
                image.width,
                image.height,
            )
        } else {
            (None, None, raw.width, raw.height)
        };
        let mut view = Box::<am_frame_view_t>::default();
        unsafe {
            args::write(
                &mut *view,
                am_frame_view_t {
                    width,
                    height,
                    format: AM_PF_RGBA8 as u32,
                    plane_count: 1,
                    planes: [
                        rgba.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
                        std::ptr::null(),
                        std::ptr::null(),
                        std::ptr::null(),
                    ],
                    strides: [width * 4, 0, 0, 0],
                    plane_size_bytes: [rgba.as_ref().map_or(0, |v| v.len() as u64), 0, 0, 0],
                    color_matrix: AM_CM_BT601 as u32,
                    color_range: AM_CR_FULL as u32,
                    ..Default::default()
                },
            )?;
        }
        Ok(Arc::new(Self {
            raw,
            rgba,
            view,
            _charge: charge,
        }))
    }
    pub fn bytes(&self) -> u64 {
        self.rgba
            .as_ref()
            .map_or(self.raw.encoded.len() as u64, |v| v.len() as u64)
    }
    pub fn kind(&self) -> u32 {
        if self.rgba.is_some() {
            AM_CAP_CPU_FRAME
        } else {
            AM_CAP_ENCODED_JPEG
        }
    }
    pub fn capture(
        state: &Arc<state::State>,
        source: &Arc<SharedSource>,
        kind: u32,
        timeout: Duration,
    ) -> Result<Arc<Self>> {
        Self::build(io::acquire(state, source, timeout)?, kind, &source.state)
    }
    pub fn continuous(
        state: &Arc<state::State>,
        source: &Arc<SharedSource>,
        kind: u32,
        timeout: Duration,
        interval: u32,
    ) -> Result<Arc<Self>> {
        Self::build(
            io::continuous(state, source, timeout, interval)?,
            kind,
            &source.state,
        )
    }
}
pub struct Output {
    _value: Box<am_captured_image_t>,
    _image: Arc<Image>,
    _charge: Charge,
    _control: ControlCharge,
}
unsafe impl Send for Output {}
unsafe impl Sync for Output {}
impl Buffer for Output {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
pub fn transfer(
    image: Arc<Image>,
    camera: &Arc<CameraState>,
    id: u64,
) -> Result<*mut am_captured_image_t> {
    let external = camera.charge(image.bytes(), true)?;
    let control = camera.control(std::mem::size_of::<Output>(), true)?;
    let mut value = Box::<am_captured_image_t>::default();
    let kind = image.kind();
    unsafe {
        args::write(
            &mut *value,
            am_captured_image_t {
                request_id: id,
                output_kind: kind,
                encoded: if kind == AM_CAP_ENCODED_JPEG {
                    image.raw.encoded.as_ptr()
                } else {
                    std::ptr::null()
                },
                encoded_size: if kind == AM_CAP_ENCODED_JPEG {
                    image.raw.encoded.len() as u64
                } else {
                    0
                },
                view: if kind == AM_CAP_CPU_FRAME {
                    &*image.view
                } else {
                    std::ptr::null()
                },
                meta: &*image.raw.meta,
                ..Default::default()
            },
        )?;
    }
    let key = (&mut *value as *mut am_captured_image_t) as usize;
    domain().buffers.insert(
        key,
        Arc::new(Output {
            _value: value,
            _image: image,
            _charge: external,
            _control: control,
        }),
    )?;
    Ok(key as *mut am_captured_image_t)
}
pub fn release(key: usize, retain: bool) -> Result<()> {
    let value = domain().buffers.get(key)?;
    if !value.as_any().is::<Output>() {
        return Err(AM_ERR_SDK_HANDLE_INVALID);
    }
    if retain {
        domain().buffers.retain(key)
    } else {
        domain().buffers.release(key)
    }
}
#[no_mangle]
pub unsafe extern "C" fn amc_image_retain(value: *mut am_captured_image_t) -> i32 {
    status(domain(), 0, || release(value as usize, true))
}
#[no_mangle]
pub unsafe extern "C" fn amc_image_release(value: *mut am_captured_image_t) -> i32 {
    status(domain(), 0, || release(value as usize, false))
}
