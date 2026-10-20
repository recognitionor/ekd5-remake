"""디컴파일 코드가 가리키는 가상 주소(VA)의 문자열·포인터 표·바이트를 원작 실행 파일에서 직접 읽음 (EUC-KR).
사용: python3 -I re/tools/pe_strings.py <EKD5.EXE> s <주소>...        문자열
      python3 -I re/tools/pe_strings.py <EKD5.EXE> p <주소> <개수>    포인터 표 → 문자열들
      python3 -I re/tools/pe_strings.py <EKD5.EXE> b <주소> <개수>    바이트
예: Ghidra 에 (&PTR_DAT_00495f78)[n] 이 보이면  p 0x495f78 11"""
import struct,sys
d=open(sys.argv[1],'rb').read()
pe=struct.unpack_from('<I',d,0x3c)[0]; ns=struct.unpack_from('<H',d,pe+6)[0]; osz=struct.unpack_from('<H',d,pe+20)[0]
ib=struct.unpack_from('<I',d,pe+24+28)[0]
secs=[struct.unpack_from('<IIII',d,pe+24+osz+40*i+8) for i in range(ns)]
def off(va):
    r=va-ib
    for vs,vaa,rs,ra in secs:
        if vaa<=r<vaa+max(vs,rs): return r-vaa+ra
def cstr(va):
    o=off(va); e=d.index(b'\0',o); return d[o:e].decode('euc-kr','replace')
def u32(va): return struct.unpack_from('<I',d,off(va))[0]
cmd=sys.argv[2]
if cmd=='s':
    for a in sys.argv[3:]: print(a, repr(cstr(int(a,16))))
elif cmd=='p':
    a=int(sys.argv[3],16); n=int(sys.argv[4])
    for i in range(n): p=u32(a+4*i); print(i, hex(p), repr(cstr(p)) if p else None)
elif cmd=='b':
    a=int(sys.argv[3],16); n=int(sys.argv[4]); print(list(d[off(a):off(a)+n]))
