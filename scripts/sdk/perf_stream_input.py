"""离线MP4样本转有界Annex-B AU；ffprobe仅备料，不参与SDK解码窗口。"""
import json
from pathlib import Path
import subprocess


def annexb(data):
    output, offset = bytearray(), 0
    while offset < len(data):
        if offset + 4 > len(data):
            raise ValueError("truncated AVCC length")
        size = int.from_bytes(data[offset:offset + 4], "big")
        offset += 4
        if size < 1 or offset + size > len(data):
            raise ValueError("invalid AVCC NAL extent")
        output += b"\0\0\0\1" + data[offset:offset + size]
        offset += size
    return bytes(output)


def has_random_access_headers(data):
    types = {nal[0] & 31 for nal in data.split(b"\0\0\0\1")[1:] if nal}
    return {5, 7, 8}.issubset(types)


def load_packets(path):
    # 自产H.264 MP4必须repeat-headers=1及4字节NAL长度，不猜未知封装。
    path = Path(path).resolve()
    if path.stat().st_size > 64 * 1024 * 1024:
        raise ValueError("input exceeds 64 MiB budget")
    probe = json.loads(subprocess.check_output(["ffprobe", "-v", "error", "-select_streams", "v:0",
        "-show_entries", "stream=codec_name,width,height,time_base,r_frame_rate,is_avc,nal_length_size,has_b_frames:packet=pos,size,pts,dts,flags",
        "-of", "json", str(path)], text=True, timeout=20))
    stream = probe["streams"][0]
    if stream["codec_name"] != "h264" or str(stream.get("nal_length_size")) != "4":
        raise ValueError("benchmark requires H.264 MP4 with four-byte NAL lengths")
    num, den = map(int, stream["time_base"].split("/"))
    if num <= 0 or den <= 0 or not 1 <= len(probe["packets"]) <= 4000:
        raise ValueError("invalid time base or packet budget")
    packets, pts = [], set()
    with path.open("rb") as source:
        for entry in probe["packets"]:
            position, size = int(entry["pos"]), int(entry["size"])
            if position < 0 or not 1 <= size <= 4 * 1024 * 1024 or position + size > path.stat().st_size:
                raise ValueError("packet extent outside sample")
            source.seek(position)
            data = annexb(source.read(size))
            timestamp = int(entry["pts"])
            if timestamp in pts:
                raise ValueError("duplicate PTS in fixture")
            pts.add(timestamp)
            packets.append(dict(data=data, pts=timestamp, dts=int(entry["dts"]), key="K" in entry["flags"]))
    if not packets[0]["key"] or not has_random_access_headers(packets[0]["data"]):
        raise ValueError("fixture must start with parameter sets and keyframe")
    if any(b["dts"] <= a["dts"] for a, b in zip(packets, packets[1:])):
        raise ValueError("DTS must be strictly increasing")
    return stream, packets
