"""EEX 디스어셈블러 시제품. 인자: u16 태그 + 값 (태그 4 = u32, 5 = C문자열, 그 외 u16). 서명의 1 = 태그 없는 u16."""
import struct, sys, glob, os
SIG = {
 0:[],1:[],2:[5],3:[],4:[0x26],5:'loop5',6:[0x2e,4]+[0x38]*5+[0x39]*5,7:[],8:[0x2e],9:[4],0xa:[],
 0xb:[4,0x27],0xc:[],0xd:[],0xe:[],0xf:[0x12],0x10:[],0x11:[0x37],0x12:[5,2],0x13:[4],0x14:[5],
 0x15:[2,2,5],0x16:[5],0x17:[5],0x18:[5],0x19:[5],0x1a:[5],0x1b:[2,0x27],0x1c:[],0x1d:[],0x1e:[],
 0x1f:[4,4],0x20:[0x4a],0x21:[4,4,0x10,0x26,0x26],0x22:[0x1b],0x23:[0x1e,4],0x24:[9],0x25:[2,4,4],
 0x26:[2,4,4,4,4],0x27:[0x2d,0xc,0x1a,0x1c,0x15],0x28:[0x36],0x29:[2,4,4],0x2a:[2,4,4],0x2b:[2],
 0x2c:[5,0x26,0x26,0x26],0x2d:[2],0x2e:[2,2,0x26],0x2f:[],0x30:[2,4,4,0x2b,0xd],0x31:[0x2c,2,4,4,4,4,3],
 0x32:[0x40,2,4,4,4,0x2b],0x33:[2,0xd,0x2b],0x34:[2,0xd],0x35:[2,0x13],0x36:[2,0x23,4,0x24],0x37:[0x28,4,0x24],
 0x38:[2,0x23,0x34,4],0x39:[2,4],0x3a:[0x28,0x34,4],0x3b:[2,0xe,0x3e],0x3c:[2,0xe,0x3a],0x3d:[0x17,0x49,0x26,2],
 0x3e:[2,0x3b,0x49,0x3c,0x49,0x3d],0x3f:[4,0x24],0x40:[0x48],0x41:[3,4,0x24,0x3f,4,4,4,4],0x42:[],0x43:[],
 0x44:[],0x45:[0x26,0x26,4,0x3e,0x32,2,0x32,2,0x47,0x22],0x46:'units46',0x47:'units47',0x48:[2,0x3b,0x49,0x3c,0x49,0x3d],
 0x49:[],0x4a:[4]+[0x38]*5+[0x39]*5,0x4b:[4,4,4,0x2b,0x26],0x4c:[0x40,2,4],0x4d:[0x41,2,4,4,4,4,4,3,0x2f,0x18,0x30,4,4],
 0x4e:[0x2c,2,4,4,4,4,3,7,2,4,4],0x4f:[2,2,0x2b,0x26,0x26,0x26],0x50:[2,0x46,0x26,0x26],0x51:[],0x52:[2,3],
 0x53:[0x2c,2,4,4,4,4,3,0x26],0x54:[],0x55:[0x40,2,4,4,4,0x2b],0x56:[0x47],0x57:[0x22],0x58:[0x42,0x43,0x44,4,4,0x26,0x26],
 0x59:[4,0x17,0x49,0x17,0x49,0x17,0x49,0x26],0x5a:[],0x5b:[4,4,4,4,0x26],0x5c:[2],0x5d:[0x34,4],0x5e:[2,2],0x5f:[],
 0x60:[0x26,5,0x4c],0x61:[],0x62:[0x26],0x63:[0x26,5,0x26],0x64:[0x26,0x4c],0x65:[0x26,0x4d,0x26],0x66:[0x26,0x4e,0x26],
 0x67:[4,5],0x68:[2,2],0x69:[5],0x6a:[],0x6b:[4,4,0x4b,0x26],
}
class R:
    def __init__(s,d,p): s.d=d; s.p=p
    def u16(s): v=struct.unpack_from('<H',s.d,s.p)[0]; s.p+=2; return v
    def u32(s): v=struct.unpack_from('<I',s.d,s.p)[0]; s.p+=4; return v
    def arg(s,t):
        if t==1: return s.u16()
        tag=s.u16()
        if tag!=t: raise ValueError(f'tag {tag:#x}!={t:#x} @{s.p-2:#x}')
        if t==4: return s.u32()
        if t==5:
            e=s.d.index(b'\0',s.p); v=s.d[s.p:e]; s.p=e+1
            try: return v.decode('cp949')
            except: return v.decode('shift_jis','replace')
        return s.u16()
def op(r,out,ind):
    at=r.p; o=r.u16()
    sig=SIG.get(o)
    if sig is None: raise ValueError(f'unknown op {o:#x} @{at:#x}')
    args=[]
    if sig=='loop5':
        for _ in range(2):
            n=r.arg(0x35); args.append(('n',n)); args+= [r.arg(1) for _ in range(n)]
    elif sig in ('units46','units47'):
        rec=[2,0x26,4,4,0x2b,0x3e,0x45,7,2,4,4] if sig=='units46' else [2,0x26,0x26,4,4,0x2b,0x3e,0x45,7,2,4,4]
        while True:
            save=r.p; tag=struct.unpack_from('<H',r.d,r.p)[0]
            if tag!=2: break
            args.append([r.arg(t) for t in rec])
    else:
        args=[r.arg(t) for t in sig]
    out.append(f'{at:05x} {"  "*ind}op{o:02x} {args}')
    return o
def body(r,out,ind,end):
    while True:
        o=op(r,out,ind)
        if o==0: return
        if o==1:
            op(r,out,ind+1)        # 조건
            n=r.u16(); e=r.p+n
            out.append(f'      {"  "*ind}{{ len {n:#x}')
            body(r,out,ind+1,e)
            if r.p!=e: raise ValueError(f'nested end {r.p:#x} != {e:#x}')
def disasm(path):
    d=open(path,'rb').read(); out=[]
    assert d[:4]==b'EEX\0'
    p=6
    while True:
        t,l=struct.unpack_from('<HH',d,p); p+=4+l
        if t==0: break
    H=p; first=struct.unpack_from('<I',d,H)[0]; n=(first-H)//4
    secs=[struct.unpack_from('<I',d,H+i*4)[0] for i in range(n)]+[len(d)]
    for si in range(n):
        r=R(d,secs[si]); nb=r.u16(); out.append(f'== section {si} blocks {nb}')
        for b in range(nb):
            bl=r.u16(); be=r.p+bl; out.append(f'-- block {b} len {bl:#x}')
            while True:          # 머리 조건
                o=op(r,out,1)
                if o==0: break
            n=r.u16(); e=r.p+n; out.append(f'   body len {n:#x}')
            body(r,out,1,e)
            if r.p!=e: raise ValueError(f'body end {r.p:#x}!={e:#x}')
            if r.p!=be: raise ValueError(f'block end {r.p:#x}!={be:#x}')
        if r.p!=secs[si+1]: raise ValueError(f'section end {r.p:#x}!={secs[si+1]:#x}')
    return out
if __name__=='__main__':
    files=sys.argv[1:]
    ok=0
    for f in files:
        try:
            out=disasm(f); ok+=1
            if len(files)==1: print('\n'.join(out))
        except Exception as e:
            print(os.path.basename(f), 'ERR', e)
    print(ok,'/',len(files))
