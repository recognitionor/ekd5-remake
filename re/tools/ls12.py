"""KOEI LS11/Ls12 압축 묶음: "Ls12"(16) + 사전 256 + 항목(BE u32 압축크기, 원래크기, 오프셋)… 0 종료"""
import struct, sys
class Bits:
    def __init__(s, d, p): s.d, s.p, s.b = d, p, 0
    def bit(s):
        v = (s.d[s.p] >> (7 - s.b)) & 1
        s.b += 1
        if s.b == 8: s.b = 0; s.p += 1
        return v
def code(bs):
    c = n = 0
    while True:
        b = bs.bit(); c = (c << 1) | b; n += 1
        if not b: break
    c2 = 0
    for _ in range(n): c2 = (c2 << 1) | bs.bit()
    return c + c2
def entries(d):
    out = []; p = 0x110
    while True:
        cs = struct.unpack_from('>I', d, p)[0]
        if cs == 0: break
        us, off = struct.unpack_from('>II', d, p + 4); out.append((cs, us, off)); p += 12
    return out
def unpack(d, cs, us, off):
    dic = d[0x10:0x110]
    if cs == us: return d[off:off+us]
    bs = Bits(d, off); o = bytearray()
    while len(o) < us:
        c = code(bs)
        if c < 256: o.append(dic[c])
        else:
            dist = c - 256; ln = code(bs) + 3
            for _ in range(ln): o.append(o[-dist])
    return bytes(o[:us])
if __name__ == '__main__':
    d = open(sys.argv[1], 'rb').read()
    es = entries(d); print(len(es), 'entries', es[:3])
    x = unpack(d, *es[0]); print(len(x), x[:64].hex())
