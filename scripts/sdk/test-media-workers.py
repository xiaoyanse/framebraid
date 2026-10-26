"""真实DLL媒体工作槽；Digest请求屏障、跨Context及关闭，默认不接现场。"""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import sys
import threading
import time

sys.dont_write_bytecode = True
from camera_recovery_fixture import module, media_workers, lifecycle, until, Stream, Session
from local_paths import local_path
t = module("test-camera-capture.py")
tables = module("test-capture-tables.py")
a, check, size = t.a, t.check, t.size


class Gate(t.Fixture):
    entered, release = threading.Event(), threading.Event()
    def do_GET(self):
        if self.headers.get("Authorization", "").startswith("Digest "):
            Gate.entered.set()
            Gate.release.wait(6)
        super().do_GET()


def stats(lib, ctx):
    value = c.POINTER(a.am_stats_t)()
    check(lib.amc_get_stats(ctx,None,c.byref(value)))
    try:
        return dict(active=value.contents.threads_active, peak=value.contents.threads_peak)
    finally:
        check(lib.amc_stats_release(value))


def limit(lib, ctx):
    value = c.c_uint32(777)
    check(lib.amc_get_worker_budget(ctx,c.byref(value)))
    return value.value


def raw_case(lib, port, baseline):
    ctx, other, source, second_source = tables.context(lib), None, None, None
    Gate.entered.clear(); Gate.release.clear()
    try:
        source = t.open_source(lib,ctx,port)
        if not baseline:
            assert limit(lib,ctx)==a.AM_MEDIA_WORKER_LIMIT
            check(lib.amc_set_worker_budget(ctx,1))
            assert limit(lib,ctx)==1
        code, first = tables.request(lib,source)
        check(code); assert first
        assert Gate.entered.wait(2)
        live = stats(lib,ctx)
        os_names = media_workers(lifecycle.windows())
        assert "amc-capture" in os_names
        if baseline:
            assert live==dict(active=0,peak=0)
        else:
            assert live==dict(active=1,peak=1)
            check(lib.amc_set_worker_budget(ctx,0),a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
            assert limit(lib,ctx)==1
            check(lib.amc_set_worker_budget(ctx,2),a.AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
            code, refused = tables.request(lib,source)
            check(code,a.AM_ERR_SDK_RESOURCE_EXHAUSTED); assert refused==0
            other = tables.context(lib)
            check(lib.amc_set_worker_budget(other,1))
            second_source = t.open_source(lib,other,port)
            code, second = tables.request(lib,second_source)
            check(code); assert second
            assert stats(lib,other)==dict(active=1,peak=1)
            assert stats(lib,ctx)==dict(active=1,peak=1)
        Gate.release.set()
        tables.fetch(lib,ctx,first)
        if other:
            tables.fetch(lib,other,second)
        until(lambda: stats(lib,ctx)["active"]==0)
        if not baseline:
            check(lib.amc_set_worker_budget(ctx,0))
            code, refused = tables.request(lib,source)
            check(code,a.AM_ERR_SDK_RESOURCE_EXHAUSTED); assert refused==0
            # 同步调用由宿主线程执行，不被当作SDK新建线程。
            image=t.captured(lib,source)
            check(lib.amc_image_release(image))
            assert stats(lib,ctx)==dict(active=0,peak=1)
        return dict(live_stats=live, os_worker_names=os_names, drained=stats(lib,ctx),
                    isolated_context=(other is not None), refused_without_new_id=not baseline,
                    synchronous_host_thread_allowed=not baseline)
    finally:
        Gate.release.set()
        if second_source: t.close_object(lib,second_source)
        if other: t.shared.close(lib,other,True)
        if source: t.close_object(lib,source)
        t.shared.close(lib,ctx,True)


def callback_case(lib, port):
    ctx, source, sub = tables.context(lib),None,None
    entered, release=threading.Event(),threading.Event()
    errors=[]
    @a.am_capture_cb_fn
    def callback(_sub,_image,_user):
        try:
            assert lib.amc_set_worker_budget(ctx,0)==a.AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION
        except BaseException as e:
            errors.append(repr(e))
        entered.set(); release.wait(3)
    try:
        source=t.open_source(lib,ctx,port)
        check(lib.amc_set_worker_budget(ctx,1))
        desc=size(a.am_capture_continuous_desc_t)
        desc.output_kind,desc.mode=a.AM_CAP_ENCODED_JPEG,a.AM_SUB_PUSH
        desc.target_interval_ms,desc.request_timeout_ms=1000,1000
        desc.queue_max_items,desc.queue_max_bytes=1,1024*1024
        sub=c.POINTER(a.amc_capture_sub_t)()
        check(lib.amc_capture_continuous_start(source,c.byref(desc),c.byref(sub)))
        check(lib.amc_capture_sub_set_callback(sub,callback,None))
        assert entered.wait(2)
        assert stats(lib,ctx)==dict(active=1,peak=1)
        state=c.c_uint32()
        check(lib.amc_capture_continuous_stop(sub,0,c.byref(state)))
        assert state.value==a.AM_LIFECYCLE_CLOSING
        assert stats(lib,ctx)["active"]==1
        release.set()
        t.close_object(lib,sub); sub=None
        until(lambda: stats(lib,ctx)["active"]==0)
        assert not errors,errors
        return dict(callback_includes_live_slot=True, stop_did_not_release_busy_worker=True,
                    unsafe_setter_reentry_rejected=True, after=stats(lib,ctx))
    finally:
        release.set()
        if sub: t.close_object(lib,sub)
        if source: t.close_object(lib,source)
        t.shared.close(lib,ctx,True)


def safe_case(dll):
    sys.path.insert(0,str(Path(__file__).resolve().parents[2]/"sdk/bindings/python"))
    from safe_sdk import CameraContext
    with CameraContext(dll) as ctx:
        assert ctx.get_worker_budget()==64
        ctx.set_worker_budget(2)
        assert ctx.get_worker_budget()==2
        try: ctx.set_worker_budget(65)
        except ValueError: pass
        else: raise AssertionError("invalid high-level budget accepted")
        ctx.set_worker_budget(0)
        assert ctx.get_worker_budget()==0
    return True


def probe_case(lib, port):
    probe = module("test-camera-probe.py")
    ctx = tables.context(lib)
    urls=[f"http://127.0.0.1:{port}/ISAPI/Streaming/channels/{channel}/picture" for channel in (101,201,301)]
    credential=size(a.am_credential_data_t)
    credential.username_utf8,credential.password_utf8=b"test",b"test"
    try:
        check(lib.amc_set_worker_budget(ctx,1))
        owned,rows=probe.probe(lib,ctx,urls,credential,force=True,timeout=1000)
        try: assert all(row["status"]==a.AM_PROBE_OK for row in rows),rows
        finally: probe.release(lib,owned)
        assert stats(lib,ctx)==dict(active=0,peak=1)
        check(lib.amc_set_worker_budget(ctx,0))
        before=t.Fixture.requests
        owned,rows=probe.probe(lib,ctx,urls,credential,force=False,timeout=1000)
        try: assert all(row["status"]==a.AM_PROBE_OK for row in rows),rows
        finally: probe.release(lib,owned)
        assert t.Fixture.requests==before
        return dict(three_candidates_with_one_slot=True,cached_results_need_no_worker=True,after=stats(lib,ctx))
    finally: t.shared.close(lib,ctx,True)


def closing_case(lib,port):
    ctx,source,image=tables.context(lib),None,None
    try:
        source=t.open_source(lib,ctx,port)
        image=t.captured(lib,source)
        t.close_object(lib,source); source=None
        check(lib.amc_set_worker_budget(ctx,1))
        state=c.c_uint32()
        check(lib.amc_shutdown(ctx,0,c.byref(state)))
        assert state.value==a.AM_LIFECYCLE_CLOSING
        assert limit(lib,ctx)==1
        check(lib.amc_set_worker_budget(ctx,0),a.AM_ERR_SDK_HANDLE_CLOSED)
        check(lib.amc_image_release(image)); image=None
        check(lib.amc_shutdown(ctx,1000,c.byref(state)))
        assert state.value==a.AM_LIFECYCLE_CLOSED
        value=c.c_uint32(777)
        check(lib.amc_get_worker_budget(ctx,c.byref(value)),a.AM_ERR_SDK_HANDLE_CLOSED)
        assert value.value==0
        check(lib.amc_object_destroy(ctx)); ctx=None
        return dict(closing_read_only=True,closing_mutation_rejected=True,closed_output_cleared=True)
    finally:
        if image: check(lib.amc_image_release(image))
        if source: t.close_object(lib,source)
        if ctx: t.shared.close(lib,ctx,True)


def rtsp_case(lib,args):
    work=local_path(args.rtsp_work.resolve(),"tests",fresh=True); work.mkdir(parents=True)
    params=size(a.amc_init_params_t); params.abi_version=a.AM_ABI_VERSION
    dirs=(c.c_char_p*1)(str(args.rtsp_runtime_dir.resolve()).encode())
    params.runtime_dirs_utf8,params.runtime_dir_count=dirs,1
    ctx=c.POINTER(a.amc_context_t)(); check(lib.amc_init(c.byref(params),c.byref(ctx)))
    rows=[]
    try:
        check(lib.amc_set_worker_budget(ctx,5))
        with Stream(work,args.mediamtx.resolve()) as stream:
            for _ in range(3):
                session=Session(lib,ctx,stream,push=True)
                try:
                    until(lambda: session.counts[0]>0 and session.counts[1]>0)
                    live=stats(lib,ctx)
                    names=media_workers(lifecycle.windows())
                    assert live["active"]==5 and live["peak"]==5,(live,names)
                    assert len(names)==5,names
                    check(lib.amc_set_worker_budget(ctx,4),a.AM_ERR_SDK_RESOURCE_EXHAUSTED)
                finally: session.close()
                until(lambda: not media_workers(lifecycle.windows()))
                assert stats(lib,ctx)==dict(active=0,peak=5)
                rows.append(dict(live=live,os_names=names,after=stats(lib,ctx)))
            check(lib.amc_set_worker_budget(ctx,4))
            limited=None
            try:
                limited=Session(lib,ctx,stream,push=True)
                until(lambda: limited.counts[0]>=3)
                until(lambda: limited.status().fault_class==a.AM_FAULT_RESOURCE)
                assert limited.counts[1]==0
                assert stats(lib,ctx)["active"]<=4
                refused=dict(stage="lazy pixels",compressed_continued=True,pixel_frames=0,stats=stats(lib,ctx))
            except AssertionError as error:
                assert error.args==((a.AM_ERR_SDK_RESOURCE_EXHAUSTED,0),),error.args
                refused=dict(stage="callback registration",synchronous_resource_error=True)
            finally:
                if limited: limited.close()
            until(lambda:not media_workers(lifecycle.windows()))
            assert stats(lib,ctx)==dict(active=0,peak=5)
        return dict(cycles=rows,limited=refused,quota_refusal_drained=True,owned_services_exited=True)
    finally: t.shared.close(lib,ctx,True)


def main(args):
    out=local_path(args.output.resolve(),"evidence",fresh=True); out.mkdir(parents=True)
    target=dict(metric="MET-2 M-OBS/M-RECOVER",worker_limit=1,independent_contexts=2,
                baseline=args.observe_baseline,timing="actual worker blocked at Digest fixture before HTTP result")
    (out/"target-card.json").write_bytes((json.dumps(target,indent=2)+"\n").encode())
    dll=args.dll_dir.resolve()/"am_camera.dll"; lib=c.CDLL(str(dll))
    missing=a.bind(lib,"amc_",require_all=not args.observe_baseline)
    if args.observe_baseline: assert set(missing)=={"amc_get_worker_budget","amc_set_worker_budget"}
    else:
        ctx=tables.context(lib)
        try:
            cap=size(a.amc_capability_t); check(lib.amc_query_capability(ctx,c.byref(cap)))
            assert cap.feature_bits&a.AMC_CAP_WORKER_BUDGET
        finally: t.shared.close(lib,ctx,True)
    server=t.ThreadingHTTPServer(("127.0.0.1",0),Gate); server.daemon_threads=True
    thread=threading.Thread(target=server.serve_forever,daemon=True); thread.start()
    try:
        result=dict(raw=raw_case(lib,server.server_address[1],args.observe_baseline))
        if not args.observe_baseline:
            result["callback"]=callback_case(lib,server.server_address[1])
            result["python_wrapper"]=safe_case(dll)
            result["probe"]=probe_case(lib,server.server_address[1])
            result["closing"]=closing_case(lib,server.server_address[1])
            if args.rtsp_runtime_dir: result["rtsp"]=rtsp_case(lib,args)
        until(lambda:not media_workers(lifecycle.windows()))
        result["dll_sha256"]=hashlib.sha256(dll.read_bytes()).hexdigest()
        (out/"result.json").write_bytes((json.dumps(result,indent=2)+"\n").encode())
        print(json.dumps(result))
    finally:
        Gate.release.set(); server.shutdown(); server.server_close(); thread.join(2)
        assert not thread.is_alive()


if __name__=="__main__":
    p=argparse.ArgumentParser()
    for name in ("dll-dir","output"): p.add_argument("--"+name,type=Path,required=True)
    p.add_argument("--observe-baseline",action="store_true")
    p.add_argument("--rtsp-runtime-dir",type=Path)
    p.add_argument("--mediamtx",type=Path)
    p.add_argument("--rtsp-work",type=Path)
    args=p.parse_args()
    if args.rtsp_runtime_dir and (not args.mediamtx or not args.rtsp_work): p.error("RTSP checks require --mediamtx and --rtsp-work")
    main(args)
