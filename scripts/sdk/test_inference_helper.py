"""GPU 对照用图像输入构造（真实 PNG 走 am_image_input_t 图像路径）。"""
import ctypes


def load_image_input(vis, a, size, image_path):
    from pathlib import Path
    payload = Path(image_path).read_bytes()
    storage = (a.c_ubyte * len(payload)).from_buffer_copy(payload)
    assoc = size(a.am_frame_assoc_t)
    assoc.source_id, assoc.frame_id = 7, 1001
    assoc.valid = a.AM_ASSOC_SOURCE | a.AM_ASSOC_FRAME
    image = size(a.am_image_input_t)
    image.kind = a.AM_IMG_JPEG if image_path.lower().endswith((".jpg", ".jpeg")) else a.AM_IMG_PNG
    image.encoded, image.encoded_size = storage, len(payload)
    image.assoc = ctypes.pointer(assoc)
    params = size(a.am_infer_params_t)
    params.confidence_threshold = 0.25
    return image, params
