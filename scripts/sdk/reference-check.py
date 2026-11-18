#!/usr/bin/env python3
"""参考输出对照器：SDK 输出 JSON 对照档案参考 JSON，按 ABI §4.2 容差表逐项判定 PASS/FAIL。

用法：
  python reference-check.py --sdk <sdk_output.json> --reference <reference_dir_or_json> [--image-dir <dir>]

SDK 输出 JSON 为 FFI harness 导出的同版式实例表：
  {"<profileId>": {"task": <int>, "instances": [...], ...}} 或单档案 {"profileId": ..., "instances": [...]}。
参考 JSON 为 crates/vision/resources/reference/<profileId>.json（schemaVersion 1）。

容差（合同定稿）：detect/segment 框中心/宽高 ≤2px（原图系）、类别一致+IoU≥0.5 配对、
top-1 一致、conf 差 ≤1e-3；classify top-1 一致、概率差 ≤1e-3；pose 关键点 ≤2px；
segment 掩膜 IoU ≥0.95（双侧带位图时算 IoU，仅带哈希时按 SHA 相等判定——严于 IoU）；obb 顶点 ≤2px、角度 ≤0.5°。
退出码：全 PASS=0，否则 1。
"""
import argparse
import json
import math
import sys
from pathlib import Path

CONF_TOL = 1e-3
PX_TOL = 2.0
ANGLE_TOL_DEG = 0.5
MASK_IOU_MIN = 0.95
PAIR_IOU_MIN = 0.5
PAIR_DIST_PX = 8.0


def fail(msg):
    print(f"FAIL {msg}")
    raise SystemExit(2)


def load_sdk(path):
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    if "instances" in data:
        return {data.get("profileId", "unknown"): data}
    return data


def load_references(arg):
    p = Path(arg)
    if p.is_dir():
        refs = {}
        for f in sorted(p.glob("*.json")):
            doc = json.loads(f.read_text(encoding="utf-8"))
            pid = doc.get("profileId", f.stem)
            refs[pid] = doc
        return refs
    doc = json.loads(p.read_text(encoding="utf-8"))
    return {doc.get("profileId", p.stem): doc}


def px(v, dim):
    return float(v) * dim


def box_iou_px(a, b, w, h):
    ax0, ay0, ax1, ay1 = (px(v, w if i % 2 == 0 else h) for i, v in enumerate(a))
    bx0, by0, bx1, by1 = (px(v, w if i % 2 == 0 else h) for i, v in enumerate(b))
    ix = max(0.0, min(ax1, bx1) - max(ax0, bx0))
    iy = max(0.0, min(ay1, by1) - max(ay0, by0))
    inter = ix * iy
    union = (ax1 - ax0) * (ay1 - ay0) + (bx1 - bx0) * (by1 - by0) - inter
    return inter / union if union > 1e-12 else 0.0


def mask_iou(a_hex, b_hex):
    inter = union = 0
    for ba, bb in zip(bytes.fromhex(a_hex), bytes.fromhex(b_hex)):
        inter += bin(ba & bb).count("1")
        union += bin(ba | bb).count("1")
    return (inter / union) if union else 1.0


def check_classify(sdk, ref, verdicts):
    si, ri = sdk["instances"], ref["instances"]
    ok = len(si) >= 1 and len(ri) >= 1
    t1 = bool(ok and si[0]["classId"] == ri[0]["classId"])
    pd = abs(si[0]["probability"] - ri[0]["probability"]) if ok else None
    verdicts.append(("classify top-1 一致", t1, f"sdk={si[0]['classId'] if si else None} ref={ri[0]['classId'] if ri else None}"))
    verdicts.append(("classify top-1 概率差 ≤1e-3", bool(pd is not None and pd <= CONF_TOL), f"diff={pd}"))
    paired = min(len(si), len(ri))
    diffs = [abs(a["probability"] - b["probability"]) for a, b in zip(si[:paired], ri[:paired])]
    cls_ok = all(a["classId"] == b["classId"] for a, b in zip(si[:paired], ri[:paired]))
    verdicts.append(("classify top-k 类别与概率对照", bool(cls_ok and (not diffs or max(diffs) <= CONF_TOL)),
                     f"paired={paired} maxProbDiff={max(diffs) if diffs else 0.0}"))


def check_pose(sdk, ref, w, h, verdicts):
    si, ri = sdk["instances"], ref["instances"]
    verdicts.append(("pose 实例数一致", len(si) == len(ri), f"sdk={len(si)} ref={len(ri)}"))
    n = min(len(si), len(ri))
    worst = 0.0
    for a, b in zip(si[:n], ri[:n]):
        for ka, kb in zip(a["keypoints"], b["keypoints"]):
            worst = max(worst, math.hypot((ka[0] - kb[0]) * w, (ka[1] - kb[1]) * h))
    verdicts.append(("pose 关键点 ≤2px", bool(n > 0 and worst <= PX_TOL), f"maxKptPx={worst:.4f}"))


def check_detect_segment(sdk, ref, profile, w, h, verdicts):
    si, ri = sdk["instances"], ref["instances"]
    verdicts.append((f"{profile} 实例数一致", len(si) == len(ri), f"sdk={len(si)} ref={len(ri)}"))
    n = min(len(si), len(ri))
    pairs = []
    for idx, (a, b) in enumerate(zip(si[:n], ri[:n])):
        cls_ok = a["classId"] == b["classId"]
        iou = box_iou_px(a["bounds"], b["bounds"], w, h)
        ac = [px(a["bounds"][0], w), px(a["bounds"][1], h), px(a["bounds"][2], w), px(a["bounds"][3], h)]
        bc = [px(b["bounds"][0], w), px(b["bounds"][1], h), px(b["bounds"][2], w), px(b["bounds"][3], h)]
        cd = max(abs((ac[0] + ac[2]) / 2 - (bc[0] + bc[2]) / 2), abs((ac[1] + ac[3]) / 2 - (bc[1] + bc[3]) / 2))
        wd = max(abs((ac[2] - ac[0]) - (bc[2] - bc[0])), abs((ac[3] - ac[1]) - (bc[3] - bc[1])))
        confd = abs(a["confidence"] - b["confidence"])
        pairs.append({"idx": idx, "classOk": cls_ok, "iou": iou, "centerPx": cd, "whPx": wd, "confDiff": confd})
    geo_ok = bool(n) and all(p["classOk"] and p["iou"] >= PAIR_IOU_MIN and p["centerPx"] <= PX_TOL and p["whPx"] <= PX_TOL for p in pairs)
    verdicts.append((f"{profile} 框中心/宽高 ≤2px+类别一致+IoU≥0.5 配对", geo_ok,
                     " ".join(f"[{p['idx']}] iou={p['iou']:.4f} c={p['centerPx']:.3f} wh={p['whPx']:.3f}" for p in pairs)))
    conf_ok = bool(n) and all(p["confDiff"] <= CONF_TOL for p in pairs)
    verdicts.append((f"{profile} conf 差 ≤1e-3", conf_ok, f"maxDiff={max((p['confDiff'] for p in pairs), default=0.0):.2e}"))
    top1 = bool(n) and pairs and pairs[0]["idx"] == 0 and si[0]["classId"] == ri[0]["classId"] and box_iou_px(si[0]["bounds"], ri[0]["bounds"], w, h) >= PAIR_IOU_MIN
    verdicts.append((f"{profile} top-1 配对一致", top1, ""))
    if profile == "segment":
        check_masks(si[:n], ri[:n], verdicts)


def check_masks(si, ri, verdicts):
    ious = []
    mode = None
    for a, b in zip(si, ri):
        if "maskBitsHex" in a and "maskBitsHex" in b:
            mode = mode or "IoU(位图)"
            ious.append(mask_iou(a["maskBitsHex"], b["maskBitsHex"]))
        elif "maskSha256" in a and "maskSha256" in b:
            mode = mode or "SHA 相等（严于 IoU≥0.95）"
            ious.append(1.0 if a["maskSha256"] == b["maskSha256"] else 0.0)
        else:
            mode = "无法判定（缺掩膜数据）"
            ious.append(0.0)
    ok = bool(ious) and all(v >= MASK_IOU_MIN for v in ious)
    verdicts.append((f"segment 掩膜 {mode}", ok, f"IoU={[round(v, 5) for v in ious]}"))


def check_obb(sdk, ref, w, h, verdicts):
    si, ri = sdk["instances"], ref["instances"]
    used = [False] * len(ri)
    pairs, unpaired_sdk = [], []
    for j, b in enumerate(ri):
        best, best_d = None, None
        for i, a in enumerate(si):
            if used[i] or a["classId"] != b["classId"]:
                continue
            d = max(math.hypot((va[0] - vb[0]) * w, (va[1] - vb[1]) * h) for va, vb in zip(a["vertices"], b["vertices"]))
            if best_d is None or d < best_d:
                best_d, best = d, i
        if best is None or best_d > PAIR_DIST_PX:
            unpaired_sdk.append(j)
            continue
        used[best] = True
        a = si[best]
        ang = abs(a["angle"] - b["angle"]) * 180.0 / math.pi
        pairs.append({"sdkIdx": best, "refIdx": j, "vPx": best_d, "angDeg": ang})
    unpaired_ref = [i for i in range(len(si)) if not used[i]]
    max_v = max((p["vPx"] for p in pairs), default=0.0)
    max_a = max((p["angDeg"] for p in pairs), default=0.0)
    verdicts.append(("obb 配对顶点 ≤2px", bool(pairs) and max_v <= PX_TOL, f"paired={len(pairs)} maxV={max_v:.4f}px"))
    verdicts.append(("obb 配对角度 ≤0.5°", bool(pairs) and max_a <= ANGLE_TOL_DEG, f"maxAng={max_a:.4f}°"))
    verdicts.append(("obb 全部参考实例配对（无未配参考项）", len(unpaired_sdk) == 0,
                     f"ref={len(ri)} sdk={len(si)} paired={len(pairs)} unpairedRef={unpaired_sdk[:10]}"))
    verdicts.append(("obb 未配 SDK 实例（信息项，跨构建抑制边界翻转时允许非零）", True,
                     f"unpairedSdk={unpaired_ref[:10]} count={len(unpaired_ref)}"))


def check_profile(pid, sdk, ref):
    verdicts = []
    img_ok = sdk.get("image", {}).get("sha256") in (None, "", ref.get("image", {}).get("sha256"))
    verdicts.append(("参考图 SHA 一致", img_ok, f"sdk={sdk.get('image', {}).get('sha256', 'n/a')[:12]} ref={ref.get('image', {}).get('sha256', '')[:12]}"))
    task = sdk.get("task", ref.get("task"))
    tf = sdk.get("transform") or {}
    if "origSize" in sdk:
        w, h = sdk["origSize"]
    elif tf.get("srcW") and tf.get("srcH"):
        w, h = tf["srcW"], tf["srcH"]
    elif sdk.get("instances") and "maskWidth" in sdk["instances"][0]:
        w, h = sdk["instances"][0]["maskWidth"], sdk["instances"][0]["maskHeight"]
    else:
        w, h = ref.get("origSize", [0, 0])
    if task == 1 or pid.startswith("pose"):
        check_pose(sdk, ref, w, h, verdicts)
    elif task == 2 or pid.startswith("detect"):
        check_detect_segment(sdk, ref, "detect", w, h, verdicts)
    elif task == 3 or pid.startswith("classify"):
        check_classify(sdk, ref, verdicts)
    elif task == 4 or pid.startswith("segment"):
        check_detect_segment(sdk, ref, "segment", w, h, verdicts)
    elif task == 5 or pid.startswith("obb"):
        check_obb(sdk, ref, w, h, verdicts)
    else:
        verdicts.append(("未知任务类型", False, f"task={task}"))
    ok = all(v[1] for v in verdicts)
    print(f"== {pid}: {'PASS' if ok else 'FAIL'}")
    for item, passed, detail in verdicts:
        print(f"   [{'PASS' if passed else 'FAIL'}] {item}" + (f"  ({detail})" if detail else ""))
    return ok


def main():
    ap = argparse.ArgumentParser(description="SDK 输出对照档案参考 JSON（ABI §4.2 容差表）")
    ap.add_argument("--sdk", required=True, help="SDK 输出 JSON（harness 同版式实例表）")
    ap.add_argument("--reference", required=True, help="参考 JSON 文件或参考目录")
    args = ap.parse_args()
    sdk_docs = load_sdk(args.sdk)
    refs = load_references(args.reference)
    matched = [pid for pid in sdk_docs if pid in refs]
    if not matched:
        fail("SDK 输出与参考 JSON 无可对照档案（profileId 不相交）")
    all_ok = True
    for pid in matched:
        all_ok = check_profile(pid, sdk_docs[pid], refs[pid]) and all_ok
    unmatched_refs = sorted(set(refs) - set(sdk_docs))
    if unmatched_refs:
        print(f"NOTE 参考目录中未被本次 SDK 输出覆盖的档案: {unmatched_refs}")
    print("REFERENCE-CHECK " + ("ALL-PASS" if all_ok else "HAS-FAIL"))
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
