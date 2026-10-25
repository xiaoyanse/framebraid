"""真实解码四格式/缩放/预算/共享视图；不连接设备。"""
import argparse
import ctypes as c
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "sdk" if (ROOT / "sdk/bindings").is_dir() else ROOT
sys.path.insert(0, str(SDK / "bindings/python"))
import amrt_sdk as a
spec = importlib.util.spec_from_file_location("helpers", Path(__file__).with_name("test-shared-frames.py"))
h = importlib.util.module_from_spec(spec)
spec.loader.exec_module(h)


def check(n, expected=0):
    assert n == expected, (n, expected)


def view(camera, frame):
    v, m = h.sized(a.am_frame_view_t), h.sized(a.am_frame_meta_t)
    check(camera.amc_frame_get_view(frame, c.byref(v)))
    check(camera.amc_frame_get_meta(frame, c.byref(m)))
    data = b"".join(c.string_at(v.planes[i], v.plane_size_bytes[i]) for i in range(v.plane_count))
    return v, m, data


def run(args):
    dll = os.add_dll_directory(str(Path(args.dll_dir).resolve()))
    camera = c.CDLL(str(Path(args.dll_dir).resolve() / "am_camera.dll"))
    a.bind(camera,"amc_")
    params = h.sized(a.amc_init_params_t)
    params.abi_version,params.feature_flags = a.AM_ABI_VERSION,a.AM_INIT_SHARED_FRAMES
    dirs = (c.c_char_p * 1)(str(Path(args.runtime_dir).resolve()).encode())
    params.runtime_dirs_utf8,params.runtime_dir_count = dirs,1
    ctx = c.POINTER(a.amc_context_t)()
    check(camera.amc_init(c.byref(params),c.byref(ctx)))
    cap = h.sized(a.amc_capability_t)
    check(camera.amc_query_capability(ctx,c.byref(cap)))
    assert cap.pixel_formats == 15 and cap.feature_bits & a.AMC_CAP_SCALE
    limits = h.sized(a.am_limits_t)
    check(camera.amc_get_limits(ctx,c.byref(limits)))
    limits.max_decoders = 1
    check(camera.amc_set_limits(ctx,c.byref(limits)))
    rows=[]
    for codec,name in [(a.AM_CODEC_H264,"one.h264"),(a.AM_CODEC_H265,"one.hevc")]:
        encoded = Path(args.samples_dir,name).read_bytes()
        reference = subprocess.check_output(["ffmpeg","-v","error","-i",str(Path(args.samples_dir,name)),"-f","rawvideo","-pix_fmt","rgba","-"])
        for fmt in [a.AM_PF_RGBA8,a.AM_PF_RGB8,a.AM_PF_BGR8,a.AM_PF_NV12]:
            cfg = h.sized(a.am_decoder_config_t)
            cfg.codec,cfg.output_pixel_format,cfg.hw_policy = codec,fmt,a.AM_DHW_SOFTWARE_ONLY
            cfg.time_base_num,cfg.time_base_den = 1,90000
            dec = c.POINTER(a.amc_decoder_t)()
            check(camera.amc_decoder_create(ctx,c.byref(cfg),c.byref(dec)))
            inp = h.sized(a.am_decoder_input_t)
            raw = (c.c_ubyte * len(encoded)).from_buffer_copy(encoded)
            inp.data,inp.size,inp.flags,inp.pts,inp.dts = raw,len(raw),a.AM_PKT_AU_COMPLETE,90000,90000
            check(camera.amc_decoder_submit(dec,c.byref(inp)))
            check(camera.amc_decoder_drain(dec))
            frame = c.POINTER(a.amc_frame_t)()
            check(camera.amc_decoder_receive(dec,0,c.byref(frame)))
            v,m,data = view(camera,frame)
            assert (v.width,v.height,v.format) == (256,144,fmt)
            if fmt == a.AM_PF_RGBA8: expected=reference
            elif fmt == a.AM_PF_RGB8: expected=bytes(x for i,x in enumerate(reference) if i % 4 != 3)
            elif fmt == a.AM_PF_BGR8:
                expected=bytes(reference[i+j] for i in range(0,len(reference),4) for j in [2,1,0])
            else:
                assert v.plane_count == 2 and v.color_matrix == a.AM_CM_BT601 and v.color_range == a.AM_CR_LIMITED
                expected=bytes([81])*(256*144)+bytes([90,240])*(256*144//4)
            error=max(abs(x-y) for x,y in zip(data,expected))
            assert len(data)==len(expected) and error<=3,(fmt,error)
            bad=c.POINTER(a.amc_frame_t)()
            check(camera.amc_frame_get_scaled(frame,0,12,0,c.byref(bad)),a.AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
            check(camera.amc_frame_get_scaled(frame,64,36,2,c.byref(bad)),a.AM_ERR_SDK_INVALID_ARGUMENT)
            if fmt == a.AM_PF_NV12:
                check(camera.amc_frame_get_scaled(frame,63,36,0,c.byref(bad)),a.AM_ERR_CAMERA_PIXEL_FORMAT_UNSUPPORTED)
            for method in [0,1]:
                scaled=c.POINTER(a.amc_frame_t)()
                check(camera.amc_frame_get_scaled(frame,64,36,method,c.byref(scaled)))
                sv,sm,sd=view(camera,scaled)
                assert (sv.width,sv.height,sv.format)==(64,36,fmt)
                assert (sm.source_id,sm.frame_id,sm.pts)==(m.source_id,m.frame_id,m.pts)
                assert sm.transform.valid==1 and sm.transform.scale_x==.25 and sm.transform.scale_y==.25
                bridge=h.sized(a.am_pixel_view_t)
                check(camera.amc_frame_as_pixel_view(scaled,c.byref(bridge)))
                assert bridge.view.contents.format==fmt
                check(camera.amc_frame_release(scaled))
                check(camera.amc_frame_release(scaled),a.AM_ERR_SDK_HANDLE_INVALID)
            state=c.c_uint32()
            check(camera.amc_decoder_close(dec,100,c.byref(state)))
            assert state.value==a.AM_LIFECYCLE_CLOSED
            # 关闭不等destroy才归还解码器配额；旧帧仍可独立读取。
            other=c.POINTER(a.amc_decoder_t)()
            check(camera.amc_decoder_create(ctx,c.byref(cfg),c.byref(other)))
            check(camera.amc_decoder_close(other,100,c.byref(state)))
            check(camera.amc_object_destroy(other))
            check(camera.amc_object_destroy(dec))
            check(camera.amc_frame_release(frame))
            rows.append({"codec":codec,"format":fmt,"referenceMaxError":error,"scaleMethods":[0,1]})
    h.close(camera,ctx,True)
    print(json.dumps({"pixels":rows,"newExports":1,"runtime":"software"}))


if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--dll-dir",required=True)
    p.add_argument("--runtime-dir",required=True)
    p.add_argument("--samples-dir",required=True)
    run(p.parse_args())
