"""Bounded integer MVP decoder. Reject unknown instructions; retain offsets/immediates."""
from collections import Counter

def leb(b,p):
    v=s=0
    while True:
        x=b[p]; p+=1; v|=(x&127)<<s
        if x<128:return v,p
        s+=7
        if s>70:raise ValueError('oversized LEB')

def sections(b):
    assert b[:8]==b'\0asm\1\0\0\0'
    p=8
    while p<len(b):
        start=p; k=b[p]; n,p=leb(b,p+1); end=p+n
        assert end<=len(b)
        yield k,b[p:end],b[start:end]
        p=end

def stripped(b):
    return b[:8]+b''.join(raw for k,_,raw in sections(b) if k!=0)

def decode(b):
    rows=[]; exports={}; types=[]; signatures=[]; memory=[]
    for k,s,_ in sections(b):
        p=0
        if k==1:
            n,p=leb(s,p)
            for _ in range(n):
                assert s[p]==0x60;p+=1
                a,p=leb(s,p);args=list(s[p:p+a]);p+=a
                z,p=leb(s,p);ret=list(s[p:p+z]);p+=z
                assert all(t in (0x7f,0x7e) for t in args+ret), 'noninteger type'
                types.append(dict(args=args,results=ret))
        elif k==2:
            n,p=leb(s,p);assert n==0,'imports forbidden'
        elif k in (4,8,9,12):
            raise ValueError(f'unexpected table/start/elements/data-count section {k}')
        elif k==3:
            n,p=leb(s,p)
            for _ in range(n):
                t,p=leb(s,p);signatures.append(t)
        elif k==5:
            n,p=leb(s,p);assert n==1
            flags,p=leb(s,p);assert flags==1,'fixed nonshared memory required'
            initial,p=leb(s,p);maximum,p=leb(s,p);assert initial==maximum
            memory=[initial,maximum]
        elif k==7:
            n,p=leb(s,p)
            for _ in range(n):
                l,p=leb(s,p);name=s[p:p+l].decode();p+=l
                kind=s[p];idx,p=leb(s,p+1)
                if kind==0:exports[idx]=name
        elif k==10:
            n,p=leb(s,p)
            for index in range(n):
                size,p=leb(s,p);end=p+size;body=s[p:end];groups,p=leb(s,p);locals_=0
                for _ in range(groups):
                    count,p=leb(s,p);assert s[p] in (0x7f,0x7e);p+=1;locals_+=count
                ops=[]
                while p<end:
                    at=p;op=s[p];p+=1;imm=[]
                    count=1 if op in (2,3,4,12,13,16,32,33,34,35,36,65,66) else 2 if 0x28<=op<=0x3e else 0
                    if 0x2a<=op<=0x2b or 0x38<=op<=0x39:raise ValueError('FP memory instruction')
                    if op==14:
                        num,p=leb(s,p);imm.append(num);count=num+1
                    if op in (0x3f,0x40):raise ValueError('memory size/grow outside policy')
                    if not count and op!=14 and not(op in (0,1,5,11,15,26,27) or 0x45<=op<=0x5a or 0x67<=op<=0x8a or op in (0xa7,0xac,0xad)):
                        raise ValueError(f'unknown/noninteger MVP opcode {op:#x}')
                    for _ in range(count):
                        start=p;v,p=leb(s,p)
                        # i32/i64.const use signed LEB, including full five/ten-byte negatives.
                        if op in (65,66):
                            width=32 if op==65 else 64
                            if s[p-1]&64:v |= -1 << ((p-start)*7)
                            v &= (1<<width)-1
                            if v & (1<<(width-1)):v-=1<<width
                        imm.append(v)
                    ops.append(dict(offset=at-(end-size),opcode=f'{op:02x}',immediates=imm))
                assert p==end
                rows.append(dict(index=index,body_bytes=size,body_hex=body.hex(),locals=locals_,instructions=len(ops),opcode_counts=dict(Counter(o['opcode'] for o in ops)),operators=ops))
    for r in rows:r['export']=exports.get(r['index']);r['signature']=types[signatures[r['index']]]
    return dict(module_bytes=len(b),code_body_bytes=sum(r['body_bytes'] for r in rows),memory_pages=memory,functions=rows)
