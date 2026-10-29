typedef void (*Run)(const void*,const void*,const void*,void*);
struct Case { const char *name; Run keel, vendor; unsigned bytes,bits,signed_lanes,operation; };
extern int printf(const char*,...);
