static uint32_t random_state=0x9e3779b9;
static uint32_t random_word(void){random_state^=random_state<<13;random_state^=random_state>>17;random_state^=random_state<<5;return random_state;}
static uint64_t read_lane(const unsigned char*p,unsigned bytes){uint64_t x=0;for(unsigned i=0;i<bytes;i++)x|=(uint64_t)p[i]<<(8*i);return x;}
static void write_lane(unsigned char*p,unsigned bytes,uint64_t x){for(unsigned i=0;i<bytes;i++)p[i]=(unsigned char)(x>>(8*i));}
static int64_t signed_lane(uint64_t x,unsigned bits){return x&(1ull<<(bits-1))?(int64_t)x-(1ll<<bits):(int64_t)x;}
