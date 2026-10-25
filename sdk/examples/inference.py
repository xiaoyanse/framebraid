"""真实 RAW Identity 推理+共享帧示例；不访问相机或业务服务。"""
import argparse
import ctypes as c
from pathlib import Path
import struct
import sys
sys.dont_write_bytecode = True
SDK = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SDK / "bindings/python"))
import amrt_sdk as a
from safe_sdk import CameraContext, VisionContext, sized


def sample():
    storage = (c.c_ubyte * 12)(*[17]*12)
    frame = sized(a.am_frame_view_t)
    frame.width, frame.height, frame.format, frame.plane_count = 2, 2, a.AM_PF_RGB8, 1
    frame.planes[0], frame.strides[0], frame.plane_size_bytes[0] = storage, 6, 12
    pixel = sized(a.am_pixel_view_t)
    pixel.magic, pixel.view = a.AM_PIXEL_VIEW_MAGIC, c.pointer(frame)
    # 合成样本的显式源/帧身份；没有媒体时间或设备时钟，不伪造有效时间。
    association = sized(a.am_frame_assoc_t)
    association.source_id, association.frame_id, association.source_generation = 1, 1, 1
    association.valid = a.AM_ASSOC_SOURCE | a.AM_ASSOC_FRAME | a.AM_ASSOC_GENERATION
    association.pts = association.captured_at_ms = association.received_at_ms = -(1 << 63)
    pixel.assoc = c.pointer(association)
    return storage, frame, pixel


def validate(result):
    outputs = result.raw_outputs()
    assert len(outputs) == 1 and outputs[0][2] == (1, 3, 2, 2)
    assert struct.unpack("<12f", outputs[0][3]) == (17,)*12


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dll-dir", required=True)
    parser.add_argument("--runtime-dir", required=True)
    parser.add_argument("--model", required=True)
    args = parser.parse_args()
    with CameraContext(Path(args.dll_dir)/"am_camera.dll", shared_frames=True) as camera, \
         VisionContext(Path(args.dll_dir)/"am_vision.dll", [args.runtime_dir], True) as vision, \
         vision.load_model(args.model) as model:
        prep = sized(a.am_raw_preprocess_t)
        prep.resize, prep.target_w, prep.target_h = a.AM_RESIZE_STRETCH, 2, 2
        model.enable_notifications()
        storage, frame, pixel = sample()
        rid, result = model.infer_pixel(pixel, preprocess=prep)
        with result:
            vision.ack(rid)
            validate(result)
        with camera.create_pool(1, 64) as publisher, vision.attach_pool(publisher.descriptor) as consumer:
            publication = publisher.publish(pixel)
            with consumer.borrow(publication) as borrowed:
                rid = model.infer_pixel(borrowed.pixel_view(), asynchronous=True, preprocess=prep)
            c.memset(storage, 0, c.sizeof(storage))
            vision.wait(rid, 10)
            with vision.fetch(rid) as result:
                vision.ack(rid)
                validate(result)
        model.enable_notifications(False)
    print("Python inference passed: CPU sync/async, fetch/ack, shared lease")


if __name__ == "__main__":
    main()
