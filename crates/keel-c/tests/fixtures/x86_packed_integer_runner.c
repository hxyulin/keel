static int64_t clamp_lane(int64_t value,unsigned bits,int is_signed) {
  int64_t low=is_signed?-(INT64_C(1)<<(bits-1)):0;
  int64_t high=(INT64_C(1)<<(is_signed?bits-1:bits))-1;
  return value<low?low:value>high?high:value;
}
static void packed_reference(const struct PackedCase*t,const unsigned char*a,
                             const unsigned char*b,unsigned char*out) {
  unsigned in=t->input_bits/8,result=t->output_bits/8;
  /* Operation numbers match the named cases in the Rust fixture table. */
  if(t->operation==13) {
    uint32_t mask=0;
    for(unsigned i=0;i<t->bytes;i++)mask|=(uint32_t)(a[i]>>7)<<i;
    write_lane(out,4,mask);
    return;
  }
  if(t->operation==12) {
    for(unsigned i=0;i<t->bytes;i++)out[i]=b[i]&128?0:a[(i&~15u)+(b[i]&15)];
    return;
  }
  if(t->operation==10||t->operation==11) {
    /* PACK instructions concatenate independently inside each 128-bit half. */
    for(unsigned block=0;block<t->bytes;block+=16)
      for(unsigned source=0;source<2;source++)
        for(unsigned lane=0;lane<16/in;lane++) {
          const unsigned char*p=(source?b:a)+block+lane*in;
          int64_t value=signed_lane(read_lane(p,in),t->input_bits);
          write_lane(out+block+source*8+lane*result,result,
                     (uint64_t)clamp_lane(value,t->output_bits,t->operation==10));
        }
    return;
  }
  for(unsigned lane=0;lane<t->bytes/result;lane++) {
    unsigned offset=lane*in;
    if(t->operation==5||t->operation==6)offset=lane*in*2;
    uint64_t x=read_lane(a+offset,in),y=read_lane(b+offset,in),value=0;
    int64_t sx=signed_lane(x,t->input_bits),sy=signed_lane(y,t->input_bits);
    switch(t->operation) {
      case 0:value=(uint64_t)clamp_lane(sx+sy,t->output_bits,1);break;
      case 1:value=(uint64_t)clamp_lane((int64_t)x+(int64_t)y,t->output_bits,0);break;
      case 2:value=(uint64_t)clamp_lane(sx-sy,t->output_bits,1);break;
      case 3:value=(uint64_t)clamp_lane((int64_t)x-(int64_t)y,t->output_bits,0);break;
      case 4:value=(x+y+1)/2;break;
      case 5: {
        int64_t x1=signed_lane(read_lane(a+offset+in,in),t->input_bits);
        int64_t y1=signed_lane(read_lane(b+offset+in,in),t->input_bits);
        /* The all-INT16_MIN pair wraps to INT32_MIN rather than saturating. */
        value=(uint64_t)(sx*sy+x1*y1);break;
      }
      case 6: {
        uint64_t x1=read_lane(a+offset+in,in);
        int64_t y1=signed_lane(read_lane(b+offset+in,in),t->input_bits);
        value=(uint64_t)clamp_lane((int64_t)x*sy+(int64_t)x1*y1,16,1);break;
      }
      case 7:value=(uint64_t)(sx*sy)>>16;break;
      case 8:value=(x*y)>>16;break;
      case 9: {
        int64_t rounded=sx*sy+16384;
        /* Floor division specifies negative rounding without a signed shift. */
        value=(uint64_t)(rounded>=0?rounded/32768:-((-rounded+32767)/32768));break;
      }
      default:return;
    }
    write_lane(out+lane*result,result,value);
  }
}

int main(void) {
  const int64_t edges[]={0,1,-1,127,128,-128,-129,255,256,32767,32768,
                         -32768,-32769,65535,65536,INT32_MAX,INT32_MIN};
  unsigned edge_count=sizeof(edges)/sizeof(*edges);
  for(unsigned c=0;c<sizeof(cases)/sizeof(*cases);c++) {
    const struct PackedCase*t=&cases[c];
    unsigned input_bytes=t->input_bits/8;
    for(unsigned test=0;test<edge_count*edge_count+64;test++) {
      unsigned char a[40],b[40],actual[40],vendor[40],expected[40],memory[40],alias[40];
      memset(a,0xa5,40);memset(b,0xa5,40);memset(actual,0xa5,40);
      memset(vendor,0xa5,40);memset(expected,0xa5,40);memset(memory,0xa5,40);
      for(unsigned lane=0;lane<t->bytes/input_bytes;lane++) {
        uint64_t x=test<edge_count*edge_count?(uint64_t)edges[test/edge_count]:random_word();
        uint64_t y=test<edge_count*edge_count?(uint64_t)edges[test%edge_count]:random_word();
        write_lane(a+1+lane*input_bytes,input_bytes,x);
        write_lane(b+1+lane*input_bytes,input_bytes,y);
      }
      if(t->operation==12)for(unsigned i=0;i<t->bytes;i++)b[1+i]=(unsigned char)(test+i*37);
      packed_reference(t,a+1,b+1,expected+1);
      t->keel(a+1,b+1,actual+1);t->vendor(a+1,b+1,vendor+1);
      kpacked_evaluations=0;t->memory(a+1,b+1,memory+1);
      unsigned evaluations=t->operation==13?1:2;
      if(kpacked_evaluations!=(int)evaluations)return 2;
      for(unsigned i=0;i<40;i++)
        if(actual[i]!=expected[i]||vendor[i]!=expected[i]||memory[i]!=expected[i]) {
          printf("%s test %u byte %u: ABI=%u vendor=%u memory=%u scalar=%u\n",
                 t->name,test,i,actual[i],vendor[i],memory[i],expected[i]);return 1;
        }
      memcpy(alias,a,40);t->memory(alias+1,b+1,alias+1);
      if(memcmp(alias+1,expected+1,t->result_bytes))return 3;
      memcpy(alias,b,40);t->memory(a+1,alias+1,alias+1);
      if(memcmp(alias+1,expected+1,t->result_bytes))return 4;
    }
  }
  printf("PASS: packed saturation, averages, products, packs, byte shuffles and sign masks\n");
  return 0;
}
