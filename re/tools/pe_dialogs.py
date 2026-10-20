"""PE(윈도 실행 파일) 대화상자 리소스 덤프 — 원작 화면 배치(컨트롤 ID·위치·글)를 읽는 데 씀.
사용: python3 -I re/tools/pe_dialogs.py <EKD5.EXE> [대화상자 번호...]  (번호 없으면 전체 목록)
DLGTEMPLATE / DLGTEMPLATEEX 모두 지원. 좌표는 대화상자 단위(DLU)."""
import struct,sys
d=open(sys.argv[1],'rb').read()
pe=struct.unpack_from('<I',d,0x3c)[0]
ns=struct.unpack_from('<H',d,pe+6)[0]; osz=struct.unpack_from('<H',d,pe+20)[0]
opt=pe+24; rva_rsrc=struct.unpack_from('<I',d,opt+96+8*2)[0]
secs=[]
for i in range(ns):
    o=opt+osz+40*i; name,vs,va,rs,ra=struct.unpack_from('<8sIIII',d,o); secs.append((va,vs,ra))
def off(rva):
    for va,vs,ra in secs:
        if va<=rva<va+max(vs,1): return rva-va+ra
base=off(rva_rsrc)
def dirent(o):
    _,_,_,_,nn,ni=struct.unpack_from('<IIHHHH',d,base+o)
    for k in range(nn+ni):
        n,t=struct.unpack_from('<II',d,base+o+16+8*k); yield n,t
def rstr(o):
    n=struct.unpack_from('<H',d,base+o)[0]; return d[base+o+2:base+o+2+2*n].decode('utf-16le')
out={}
for tn,tt in dirent(0):
    if tn!=5: continue
    for nn,nt in dirent(tt&0x7fffffff):
        name = rstr(nn&0x7fffffff) if nn&0x80000000 else nn
        for ln,lt in dirent(nt&0x7fffffff):
            rva,sz=struct.unpack_from('<II',d,base+lt)[:2]; out[name]=d[off(rva):off(rva)+sz]
def sz_or_ord(b,p):
    w=struct.unpack_from('<H',b,p)[0]
    if w==0xffff: return struct.unpack_from('<H',b,p+2)[0],p+4
    if w==0: return '',p+2
    e=p
    while struct.unpack_from('<H',b,e)[0]: e+=2
    return b[p:e].decode('utf-16le'),e+2
CLS={0x80:'BUTTON',0x81:'EDIT',0x82:'STATIC',0x83:'LIST',0x84:'SCROLL',0x85:'COMBO'}
def dumpex(name,b):
    _,_,hid,ex,style,n,x,y,cx,cy=struct.unpack_from('<HHIIIHhhhh',b,0); p=26
    menu,p=sz_or_ord(b,p); cls,p=sz_or_ord(b,p); title,p=sz_or_ord(b,p)
    if style&0x40: p+=6; _,p=sz_or_ord(b,p)
    print(f"== DIALOG {name} {cx}x{cy} '{title}' n={n} EX")
    for i in range(n):
        p=(p+3)&~3
        hid,ex,st,x,y,cx,cy,cid=struct.unpack_from('<IIIhhhhI',b,p); p+=24
        c,p=sz_or_ord(b,p); t,p=sz_or_ord(b,p)
        ext=struct.unpack_from('<H',b,p)[0]; p+=2+ext
        print(f"  {cid:#x} {CLS.get(c,c)} ({x},{y},{cx},{cy}) st={st:#x} '{t}'")
def dump(name,b):
    if b[:4]==b'\x01\x00\xff\xff': return dumpex(name,b)
    style,ex,n,x,y,cx,cy=struct.unpack_from('<IIHhhhh',b,0); p=18
    menu,p=sz_or_ord(b,p); cls,p=sz_or_ord(b,p); title,p=sz_or_ord(b,p)
    if style&0x40: p+=2; _,p=sz_or_ord(b,p)
    print(f"== DIALOG {name} {cx}x{cy} '{title}' n={n}")
    for i in range(n):
        p=(p+3)&~3
        st,ex,x,y,cx,cy,cid=struct.unpack_from('<IIhhhhH',b,p); p+=18
        c,p=sz_or_ord(b,p); t,p=sz_or_ord(b,p)
        ext=struct.unpack_from('<H',b,p)[0]; p+=2+ext
        print(f"  {cid:#x} {CLS.get(c,c)} ({x},{y},{cx},{cy}) st={st:#x} '{t}'")
want=sys.argv[2:] 
for k,v in out.items():
    if not want or str(k) in want or (isinstance(k,int) and hex(k) in want): dump(k,v)
