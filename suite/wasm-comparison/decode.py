"""Bounded integer MVP decoder. Reject unknown instructions; retain offsets/immediates."""
from collections import Counter

def leb(b,p,width=32,signed=False):
    v=0;start=p
    for shift in range(0,width,7):
        if p>=len(b):raise ValueError('truncated LEB')
        x=b[p];p+=1;v|=(x&127)<<shift
        if x<128:
            if signed and x&64:v-=1<<((p-start)*7)
            lo=-(1<<(width-1)) if signed else 0
            hi=(1<<(width-1))-1 if signed else (1<<width)-1
            if not lo<=v<=hi:raise ValueError('out-of-range LEB')
            return v,p
    raise ValueError('oversized LEB')

def sections(b):
    assert b[:8]==b'\0asm\1\0\0\0'
    seen=set();last=0
    p=8
    while p<len(b):
        start=p; k=b[p]; n,p=leb(b,p+1); end=p+n
        if end>len(b):raise ValueError('section exceeds module')
        if k:
            if k>12 or k in seen or k<last:raise ValueError('unknown/duplicate/unordered section')
            seen.add(k);last=k
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
                z,p=leb(s,p);assert z<=1,'multi-value outside MVP';ret=list(s[p:p+z]);p+=z
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
        elif k==6:
            n,p=leb(s,p)
            for _ in range(n):
                ty=s[p];mutable=s[p+1];p+=2
                assert ty in (0x7f,0x7e) and mutable in (0,1),'noninteger/invalid global'
                assert s[p]==(0x41 if ty==0x7f else 0x42),'non-MVP global initializer'
                _,p=leb(s,p+1,32 if ty==0x7f else 64,True)
                assert s[p]==0x0b,'invalid global initializer end';p+=1
        elif k==7:
            n,p=leb(s,p)
            for _ in range(n):
                l,p=leb(s,p);name=s[p:p+l].decode();p+=l
                kind=s[p];idx,p=leb(s,p+1)
                if kind==0:exports[idx]=name
        elif k==10:
            n,p=leb(s,p)
            assert n==len(signatures),'function/code count mismatch'
            for index in range(n):
                size,p=leb(s,p);end=p+size
                if end>len(s):raise ValueError('body exceeds code section')
                body=s[p:end];p=end;q=0;groups,q=leb(body,q);locals_=0
                for _ in range(groups):
                    count,q=leb(body,q)
                    if q>=len(body) or body[q] not in (0x7f,0x7e):raise ValueError('invalid local type')
                    q+=1;locals_+=count
                ops=[]
                while q<len(body):
                    at=q;op=body[q];q+=1;imm=[]
                    count=1 if op in (2,3,4,12,13,16,32,33,34,35,36,65,66) else 2 if 0x28<=op<=0x3e else 0
                    if 0x2a<=op<=0x2b or 0x38<=op<=0x39:raise ValueError('FP memory instruction')
                    if op==14:
                        num,q=leb(body,q);imm.append(num);count=num+1
                        if count>len(body)-q:raise ValueError('truncated branch table')
                    if op in (0x3f,0x40):raise ValueError('memory size/grow outside policy')
                    if not count and op!=14 and not(op in (0,1,5,11,15,26,27) or 0x45<=op<=0x5a or 0x67<=op<=0x8a or op in (0xa7,0xac,0xad)):
                        raise ValueError(f'unknown/noninteger MVP opcode {op:#x}')
                    for _ in range(count):
                        if op in (2,3,4):
                            if q>=len(body) or body[q] not in (0x40,0x7f,0x7e):raise ValueError('noninteger/non-MVP block type')
                            v=body[q];q+=1
                        else:v,q=leb(body,q,64 if op==66 else 32,op in (65,66))
                        imm.append(v)
                    ops.append(dict(offset=at,opcode=f'{op:02x}',immediates=imm))
                rows.append(dict(index=index,body_bytes=size,body_hex=body.hex(),locals=locals_,instructions=len(ops),opcode_counts=dict(Counter(o['opcode'] for o in ops)),operators=ops))
        if k in (1,2,3,5,6,7,10) and p!=len(s):raise ValueError('trailing/truncated section payload')
    assert len(rows)==len(signatures),'missing code bodies'
    for r in rows:r['export']=exports.get(r['index']);r['signature']=types[signatures[r['index']]]
    return dict(module_bytes=len(b),code_body_bytes=sum(r['body_bytes'] for r in rows),memory_pages=memory,functions=rows)
