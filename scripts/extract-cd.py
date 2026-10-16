#!/usr/bin/env python3
"""원작 CD 이미지(CloneCD .ccd/.img)에서 음악 트랙을 꺼내 data/BGMnn.m4a 로 저장.
데이터 트랙(ISO9660)에서 설치본에 없는 자료(MCALL09.. 등 .E5)를 data/ 에 넣고, 동영상 .AVI(Cinepak)는 ffmpeg 로 .mp4 로 바꿈.
사용: python3 -I scripts/extract-cd.py <이미지.ccd> <data 폴더>
트랙 번호 = 원작 op24 인자 + 2 (KOEICDA.DLL 로 CD 트랙 재생). macOS afconvert 로 AAC 변환."""
import os, re, struct, subprocess, sys, tempfile

ccd, out = sys.argv[1], sys.argv[2]
img = os.path.splitext(ccd)[0] + ".img"
text = open(ccd, encoding="latin-1").read()
tracks = {}
for m in re.finditer(r"\[Entry \d+\][^\[]*", text):
    e = dict(l.split("=", 1) for l in m.group(0).splitlines()[1:] if "=" in l)
    p = int(e["Point"], 16)
    if p <= 0x63:
        tracks[p] = (int(e["PLBA"]), int(e["Control"], 16))
    elif p == 0xA2:
        lead_out = int(e["PLBA"])
nums = sorted(tracks)
SECTOR = 2352
with open(img, "rb") as f:
    for i, n in enumerate(nums):
        lba, ctl = tracks[n]
        if ctl & 4:  # 데이터 트랙
            continue
        end = tracks[nums[i + 1]][0] if i + 1 < len(nums) else lead_out
        f.seek(lba * SECTOR)
        pcm = f.read((end - lba) * SECTOR)
        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as w:
            w.write(b"RIFF" + struct.pack("<I", 36 + len(pcm)) + b"WAVEfmt " + struct.pack("<IHHIIHH", 16, 1, 2, 44100, 176400, 4, 16) + b"data" + struct.pack("<I", len(pcm)))
            w.write(pcm)
            tmp = w.name
        dst = os.path.join(out, f"BGM{n:02}.m4a")
        subprocess.run(["afconvert", "-f", "m4af", "-d", "aac", "-b", "128000", tmp, dst], check=True)
        os.unlink(tmp)
        print(dst, f"{len(pcm) / 176400:.0f}초")

# ---------- 데이터 트랙 ----------
def sec(f, n):
    f.seek(n * SECTOR)
    r = f.read(SECTOR)
    return r[16:16 + 2048] if r[15] == 1 else r[24:24 + 2048]

with open(img, "rb") as f:
    pvd = sec(f, 16)
    root = pvd[156:190]
    lba, ln = struct.unpack_from("<I", root, 2)[0], struct.unpack_from("<I", root, 10)[0]
    d = b"".join(sec(f, lba + i) for i in range((ln + 2047) // 2048))
    o = 0
    while o < len(d):
        l = d[o]
        if l == 0:
            o = (o // 2048 + 1) * 2048
            continue
        e = d[o:o + l]
        o += l
        el, sz = struct.unpack_from("<I", e, 2)[0], struct.unpack_from("<I", e, 10)[0]
        name = e[33:33 + e[32]].decode("latin-1").split(";")[0]
        if e[25] & 2 or name in ("\0", "\1"):
            continue
        ext = os.path.splitext(name)[1].upper()
        if ext not in (".E5", ".AVI") or name.startswith("PM"):  # PMnn = HMnn 과 같은 파일
            continue
        data = b"".join(sec(f, el + i) for i in range((sz + 2047) // 2048))[:sz]
        if ext == ".E5":
            dst = os.path.join(out, name)
            if not os.path.exists(dst):
                open(dst, "wb").write(data)
                print(dst)
        else:
            with tempfile.NamedTemporaryFile(suffix=".avi", delete=False) as t:
                t.write(data)
                tmp = t.name
            dst = os.path.join(out, os.path.splitext(name)[0] + ".mp4")
            subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", tmp, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "20", "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart", dst], check=True)
            os.unlink(tmp)
            print(dst)
