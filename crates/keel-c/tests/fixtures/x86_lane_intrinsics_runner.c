/* This byte oracle never performs floating arithmetic, even for NaN payloads. */
static void expected(const struct Case *c,U *r,const U *a,const U *b) {
  for(unsigned j=0;j<c->bytes;++j) {
    unsigned k=j/c->lane, byte=j%c->lane, block=j/16*16, local=j%16;
    unsigned n=c->control, index=j; const U *p=a; int zero=0;
    switch(c->op) {
    case 0: if((n>>(k%8))&1) p=b; break;
    case 1: zero=n>local; index=block+local-n; break;
    case 2: zero=n+local>=16; index=block+local+n; break;
    case 3: zero=n+local>=32; p=n+local<16?b:a; index=block+(n+local)%16; break;
    case 4: case 5: {
      unsigned word=local/2, base=c->op==5?4:0;
      if(word>=base && word<base+4) index=block+2*(base+((n>>(2*(word-base)))&3))+byte;
      break;
    }
    case 6: index=block+c->lane*((n>>((c->lane==4?2:1)*(k%(16/c->lane))))&((16/c->lane)-1))+byte; if(c->lane==8 && c->bytes==32) index=block+8*((n>>k)&1)+byte; break;
    case 7: {
      Q bits=0; for(unsigned q=0;q<c->lane;++q) bits|=(Q)b[k*c->lane+q]<<(8*q);
      index=block+c->lane*((bits>>(c->lane==8?1:0))&((16/c->lane)-1))+byte; break;
    }
    case 8: p=k%2?b:a; index=block+8*((n>>k)&1)+byte; break;
    case 9: { unsigned selection=(n>>(4*(j/16)))&15; zero=selection&8; p=(selection&3)<2?a:b; index=16*(selection&1)+local; break; }
    case 10: index=8*((n>>(2*k))&3)+byte; break;
    case 11: { unsigned bits=0; for(unsigned q=0;q<4;++q) bits|=(unsigned)b[k*4+q]<<(8*q); index=4*(bits&7)+byte; break; }
    }
    r[j]=zero?0:p[index];
  }
}
int main(void) {
  U a[32],b[32],actual[48],reference[32],want[32],alias[32];
  for(unsigned iteration=0;iteration<12;++iteration) {
    for(unsigned j=0;j<32;++j) {a[j]=(U)(j*37+iteration*93);b[j]=(U)(j*71+iteration*19+128);}
    if(iteration==0) {
      const U special[16]={1,0,128,127,255,255,255,255,0,0,0,128,1,0,240,127};
      memcpy(a,special,16); memcpy(a+16,special,16);
    }
    for(unsigned i=0;i<sizeof(cases)/sizeof(cases[0]);++i) {
      const struct Case *c=&cases[i]; expected(c,want,a,b);
      for(unsigned j=0;j<48;++j) actual[j]=0xa5;
      if(!c->actual(actual+8,a,b) || !c->reference(reference,a,b) || memcmp(want,actual+8,c->bytes) || memcmp(want,reference,c->bytes)) {printf("lane mismatch case %u iteration %u\n",i,iteration);return 1;}
      for(unsigned j=0;j<8;++j) if(actual[j]!=0xa5 || actual[8+c->bytes+j]!=0xa5) return 2;
      memcpy(alias,a,32); if(!c->actual(alias,alias,b) || memcmp(alias,want,c->bytes)) return 3;
      memcpy(alias,b,32); if(!c->actual(alias,a,alias) || memcmp(alias,want,c->bytes)) return 4;
    }
  }
  for(int index=0;index<4;++index) {
    V input={(long long)0x0123456789abcdefULL,(long long)0xfedcba9876543210ULL,42,-19};
    if(dynamic_extract(input,index)!=input[index] || ref_dynamic_extract(input,index)!=input[index]) return 5;
  }
  return 0;
}
