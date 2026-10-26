"""MET-2 S2回环RTSP真实节拍/混合消费者，配置上限不冒充实际交付率。"""
import argparse
import ctypes as c
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

from perf_common import dll_build_identity, file_sha256, summarize
from perf_resources import ResourceSampler, available_parallelism, cpu_delta, cpu_snapshot
from perf_met2 import review_sampling
from local_paths import local_path

ROOT=Path(__file__).resolve().parents[2]
sys.dont_write_bytecode=True
sys.path.insert(0,str(ROOT/"sdk/bindings/python"))
import amrt_sdk as a


def sized(kind):
    value=kind(); value.struct_size=c.sizeof(value)
    return value


def checked(code):
    if code!=a.AM_OK: raise RuntimeError(f"SDK error {code}")


def stop(process):
    if process is not None and process.poll() is None:
        process.terminate()
        try: process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill(); process.wait(timeout=5)


def close(lib,obj,method):
    state=c.c_uint32()
    checked(method(obj,2000,c.byref(state)))
    if state.value!=a.AM_LIFECYCLE_CLOSED: raise RuntimeError("source/subscription close did not drain")
    checked(lib.amc_object_destroy(obj))


def subscribe(lib,function,source,desc,pointer):
    deadline=time.monotonic()+8
    while True:
        code=function(source,c.byref(desc),c.byref(pointer))
        if code!=a.AM_ERR_CAMERA_PROBE_PENDING:
            checked(code); return
        if time.monotonic()>deadline: raise TimeoutError("source metadata not ready")
        time.sleep(.01)


def report(lib,source):
    size=c.c_uint64()
    checked(lib.amc_get_decode_hardware_report(source,None,0,c.byref(size)))
    if not 1<=size.value<=65536: raise RuntimeError("unbounded source report")
    data=c.create_string_buffer(size.value)
    checked(lib.amc_get_decode_hardware_report(source,data,size.value,c.byref(size)))
    return json.loads(data.value)


def stats_snapshot(lib,ctx):
    pointer=c.POINTER(a.am_stats_t)(); checked(lib.amc_get_stats(ctx,None,c.byref(pointer)))
    try:
        return {key:getattr(pointer.contents,key) for key in ("frames_decoded","frames_dropped","dropped",
            "reconnect_count","resync_count","threads_active","threads_peak","external_held_items")}
    finally: checked(lib.amc_stats_release(pointer))


def run_case(lib,args,url,rates):
    paths=(c.c_char_p*1)(str(args.runtime_dir.resolve()).encode())
    params=sized(a.amc_init_params_t)
    params.abi_version=a.AM_ABI_VERSION; params.runtime_dirs_utf8=paths; params.runtime_dir_count=1
    ctx=c.POINTER(a.amc_context_t)(); checked(lib.amc_init(c.byref(params),c.byref(ctx)))
    source=c.POINTER(a.amc_source_t)(); encoded=c.POINTER(a.amc_encoded_sub_t)(); subscriptions=[]
    sampler=None
    try:
        desc=sized(a.am_source_desc_t)
        desc.kind=a.AM_SRC_RTSP_URL; desc.url_utf8=url.encode(); desc.stream_kind_utf8=b"main"; desc.reuse=1
        # 隔离服务不校验账号；现有SDK源合同仍要求显式凭据拥有者。
        credentials=sized(a.am_credential_data_t)
        credentials.username_utf8=b"met2-local"; credentials.password_utf8=b"not-a-field-secret"
        desc.creds=c.pointer(credentials)
        checked(lib.amc_source_open(ctx,c.byref(desc),c.byref(source)))
        checked(lib.amc_source_set_decode_policy(source,a.AM_DHW_SOFTWARE_ONLY))
        encoded_desc=sized(a.am_encoded_sub_desc_t)
        encoded_desc.codec_mask=a.AM_CODEC_H264; encoded_desc.mode=a.AM_SUB_PULL
        encoded_desc.queue_max_items=36; encoded_desc.queue_max_bytes=8*1024*1024
        subscribe(lib,lib.amc_subscribe_encoded,source,encoded_desc,encoded)
        for rate in rates:
            frame_desc=sized(a.am_frame_sub_desc_t)
            frame_desc.pixel_format=a.AM_PF_RGBA8; frame_desc.sampling_max_fps=rate
            frame_desc.mode=a.AM_SUB_PULL; frame_desc.queue_max_items=4; frame_desc.queue_max_bytes=32*1024*1024
            sub=c.POINTER(a.amc_frame_sub_t)()
            subscribe(lib,lib.amc_subscribe_frames,source,frame_desc,sub)
            subscriptions.append(sub)
        source_identity=None; encoded_rows=[]; frame_rows=[[] for _ in rates]
        all_ids=[set() for _ in rates]; encoded_seq=set(); last_pts=[None for _ in rates]
        warmed=False; before=None; start=time.perf_counter()
        parallelism,method=available_parallelism()
        while time.perf_counter()-start<args.warmup_s+args.window_s:
            if not warmed and time.perf_counter()-start>=args.warmup_s:
                sampler=ResourceSampler().start()
                # 资源计数器初始化不挤进固定稳态窗口。
                start=time.perf_counter()-args.warmup_s
                before=cpu_snapshot(); warmed=True
            progressed=False
            packet=c.POINTER(a.am_encoded_packet_t)()
            code=lib.amc_encoded_pull(encoded,0,c.byref(packet))
            if code not in (a.AM_ERR_SDK_TRY_AGAIN,a.AM_ERR_SDK_DEADLINE_EXCEEDED):
                checked(code)
                try:
                    data=packet.contents; identity=(data.source_id,data.source_generation)
                    if data.seq in encoded_seq: raise RuntimeError("duplicate compressed packet")
                    encoded_seq.add(data.seq)
                    if source_identity is not None and identity!=source_identity: raise RuntimeError("source generation changed")
                    source_identity=identity
                    if warmed: encoded_rows.append(dict(seq=data.seq,pts=data.pts,dts=data.dts,flags=data.flags))
                finally: checked(lib.amc_encoded_packet_release(packet))
                progressed=True
            for index,sub in enumerate(subscriptions):
                frame=c.POINTER(a.amc_frame_t)()
                code=lib.amc_frame_pull(sub,0,c.byref(frame))
                if code in (a.AM_ERR_SDK_TRY_AGAIN,a.AM_ERR_SDK_DEADLINE_EXCEEDED): continue
                checked(code)
                try:
                    meta,view=sized(a.am_frame_meta_t),sized(a.am_frame_view_t)
                    checked(lib.amc_frame_get_meta(frame,c.byref(meta))); checked(lib.amc_frame_get_view(frame,c.byref(view)))
                    if (view.width,view.height,view.format)!=(1920,1080,a.AM_PF_RGBA8): raise RuntimeError("wrong frame format")
                    if source_identity is not None and (meta.source_id,meta.source_generation)!=source_identity: raise RuntimeError("frame source mismatch")
                    if meta.frame_id in all_ids[index] or (last_pts[index] is not None and meta.pts<=last_pts[index]): raise RuntimeError("duplicate/reordered decoded frame")
                    all_ids[index].add(meta.frame_id); last_pts[index]=meta.pts
                    age_ms=time.perf_counter_ns()/1e6-meta.received_at_ms
                    if age_ms<0: raise RuntimeError("QPC clock domain mismatch")
                    if warmed: frame_rows[index].append(dict(frame_id=meta.frame_id,pts=meta.pts,seq=meta.seq,
                        received_at_ms=meta.received_at_ms,consumed_at_ns=time.perf_counter_ns(),age_ms=age_ms,
                        pixel_prefix_sha256=hashlib.sha256(c.string_at(view.planes[0],16)).hexdigest()))
                finally: checked(lib.amc_frame_release(frame))
                progressed=True
            if not progressed: time.sleep(.001)
        after=cpu_snapshot(); resources=cpu_delta(before,after,parallelism)
        observation=sampler.stop(); sampler=None
        actual=report(lib,source)
        if not rates and (actual["decoder_active"] or actual["actual_backend"] is not None): raise RuntimeError("compressed-only unexpectedly decoded")
        stats=stats_snapshot(lib,ctx)
        # 活流队列内的共享Frame也占external额度；停源后才能核归还残留。
        for sub in reversed(subscriptions): close(lib,sub,lib.amc_frame_unsubscribe)
        subscriptions.clear()
        close(lib,encoded,lib.amc_encoded_unsubscribe); encoded=c.POINTER(a.amc_encoded_sub_t)()
        close(lib,source,lib.amc_source_close); source=c.POINTER(a.amc_source_t)()
        closed=stats_snapshot(lib,ctx)
        if closed["external_held_items"]: raise RuntimeError("unreturned frame/packet after source drained")
        return dict(rates=rates,planned_window_s=args.window_s,cpu=dict(resources,parallelism_method=method),observation=observation,
            compressed_packets=len(encoded_rows),compressed_fps=len(encoded_rows)/resources["wall_s"],encoded=encoded_rows,
            consumers=[dict(max_fps=rate,delivered=len(rows),effective_fps=len(rows)/resources["wall_s"],
                age=summarize([row["age_ms"]*1e6 for row in rows]),frames=rows) for rate,rows in zip(rates,frame_rows)],
            actual=actual,stats=stats,after_source_close=closed,unknown=dict(native_conversion_count=None,exposure_to_display=None),
            verdict="same-build actual sampling baseline; no before/after optimization claim")
    finally:
        try:
            if sampler is not None: sampler.stop()
            for sub in reversed(subscriptions): close(lib,sub,lib.amc_frame_unsubscribe)
            if encoded: close(lib,encoded,lib.amc_encoded_unsubscribe)
        finally:
            try:
                if source: close(lib,source,lib.amc_source_close)
            finally: close(lib,ctx,lib.amc_shutdown)


def main(args):
    output=local_path(str(args.output.resolve()),"evidence",fresh=True); output.mkdir(parents=True)
    work=local_path(str(args.work_dir.resolve()),"tests",fresh=True); work.mkdir(parents=True)
    if not .5<=args.warmup_s<=3 or not 2<=args.window_s<=8 or not 1<=args.rounds<=3: raise ValueError("experiment budget exceeded")
    with socket.socket() as sock: sock.bind(("127.0.0.1",0)); port=sock.getsockname()[1]
    config=work/"mediamtx.yml"
    config.write_text(f"logLevel: warn\nrtspAddress: 127.0.0.1:{port}\nrtspTransports: [tcp]\nrtmp: no\nhls: no\nwebrtc: no\nsrt: no\nmoq: no\npaths:\n  all_others:\n",encoding="utf-8")
    server=publisher=None; search=os.add_dll_directory(str(args.dll_dir.resolve()))
    try:
        with (work/"server.log").open("wb") as server_log,(work/"publisher.log").open("wb") as publisher_log:
            server=subprocess.Popen([str(args.mediamtx.resolve()),str(config)],stdout=server_log,stderr=subprocess.STDOUT,cwd=work)
            deadline=time.monotonic()+8
            while True:
                if server.poll() is not None: raise RuntimeError("isolated MediaMTX exited")
                try:
                    with socket.create_connection(("127.0.0.1",port),.2): break
                except OSError:
                    if time.monotonic()>deadline: raise TimeoutError("isolated RTSP server not ready")
                    time.sleep(.05)
            url=f"rtsp://127.0.0.1:{port}/met2"
            publisher=subprocess.Popen(["ffmpeg","-nostdin","-hide_banner","-loglevel","error","-re","-stream_loop","-1",
                "-i",str(args.input.resolve()),"-an","-c:v","copy","-f","rtsp","-rtsp_transport","tcp",url],stdout=publisher_log,stderr=subprocess.STDOUT)
            time.sleep(.8)
            if publisher.poll() is not None: raise RuntimeError("publisher failed")
            dll=args.dll_dir.resolve()/"am_camera.dll"; lib=c.CDLL(str(dll)); a.bind(lib,"amc_",require_all=True)
            version=sized(a.am_version_info_t); checked(lib.amc_get_version_info(c.byref(version)))
            names=(Path(__file__).name,"perf_resources.py","perf_common.py","local_paths.py","perf_met2.py")
            identity=dict(schema="framebraid.performance.met2-rtsp/1",build=dll_build_identity(dll,version.package_version_utf8.decode(),
                version.build_target_utf8.decode(),version.runtime_notes_utf8.decode(),subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),
                "dirty" if subprocess.check_output(["git","status","--porcelain"],cwd=ROOT) else "clean"),
                input_sha256=file_sha256(args.input),target_card_sha256=file_sha256(args.target_card),
                runtime_files={p.name:file_sha256(p) for p in sorted(args.runtime_dir.glob("*.dll"))},
                observer_files={name:file_sha256(Path(__file__).with_name(name)) for name in names},
                protocol=dict(warmup_s=args.warmup_s,window_s=args.window_s,rounds=args.rounds,cases=args.cases,
                              transport="TCP loopback; real-time publisher; no exposure clock"))
            (output/"identity.json").write_text(json.dumps(identity,indent=2)+"\n",encoding="utf-8")
            sources=output/"observer-sources"; sources.mkdir()
            for name in names: (sources/name).write_bytes(Path(__file__).with_name(name).read_bytes())
            summary=[]
            for index in range(args.rounds):
                for case in args.cases[::1 if index%2==0 else -1]:
                    rates=[] if case=="encoded" else [int(rate) for rate in case.split(",")]
                    try:
                        row=run_case(lib,args,url,rates)
                        row["experimental_review"]=review_sampling(row)
                    except Exception as error:
                        (output/"failure.json").write_text(json.dumps(dict(case=case,round=index+1,error=str(error))),encoding="utf-8"); raise
                    name=f"case-{case.replace(',','-')}-{index+1}.json"
                    (output/name).write_text(json.dumps(row,indent=2)+"\n",encoding="utf-8")
                    summary.append(dict(file=name,cpu_s=row["cpu"]["cpu_s"],compressed_fps=row["compressed_fps"],
                        consumers=[dict(max_fps=r["max_fps"],effective_fps=r["effective_fps"],age_p95_ms=r["age"]["p95_ms"]) for r in row["consumers"]]))
            (output/"summary.json").write_text(json.dumps(summary,indent=2)+"\n",encoding="utf-8")
            print(json.dumps(dict(output=str(output),summary=summary),indent=2))
    finally:
        stop(publisher); stop(server); search.close()


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ("dll-dir","runtime-dir","input","mediamtx","output","work-dir","target-card"): parser.add_argument("--"+name,type=Path,required=True)
    parser.add_argument("--warmup-s",type=float,default=2); parser.add_argument("--window-s",type=float,default=4)
    parser.add_argument("--rounds",type=int,default=3)
    parser.add_argument("--cases",nargs="+",choices=("encoded","5","10","30","5,10,30"),default=["encoded","5","10","30","5,10,30"])
    main(parser.parse_args())
