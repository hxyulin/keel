#define _GNU_SOURCE
#include <link.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static unsigned done;
static uint64_t ready[4]={UINT64_C(0x4b45454c50414745),0,0,0};
static void fail(const char *why){dprintf(2,"page-backing: %s\n",why);_exit(120);}
static int visit(struct dl_phdr_info *info,size_t size,void *unused){
    (void)size;(void)unused;
    if(info->dlpi_name && info->dlpi_name[0])return 0;
    const char *pool=getenv("KEEL_TEXT_POOL"),*plan=getenv("KEEL_TEXT_PLAN");
    if(!pool || !plan)fail("missing plan");
    int source=open("/proc/self/exe",O_RDONLY|O_CLOEXEC);
    int backing=open(pool,O_RDONLY|O_CLOEXEC),layout=open(plan,O_RDONLY|O_CLOEXEC);
    if(source<0 || backing<0 || layout<0)fail("open");
    struct stat ls,bs,ss;
    if(fstat(layout,&ls)||fstat(backing,&bs)||fstat(source,&ss)||
       ls.st_size<8||ls.st_size>65536||ls.st_size%8)fail("plan size");
    uint64_t *indices=malloc(ls.st_size);
    if(!indices || read(layout,indices,ls.st_size)!=ls.st_size)fail("plan read");
    if(sysconf(_SC_PAGESIZE)!=4096)fail("page size");
    for(ElfW(Half) i=0;i<info->dlpi_phnum;i++){
        const ElfW(Phdr)*ph=&info->dlpi_phdr[i];
        if(ph->p_type!=PT_LOAD || !(ph->p_flags&PF_X))continue;
        if(done || (ph->p_flags&PF_W)||ph->p_memsz!=ph->p_filesz)fail("unsupported RX segment");
        uintptr_t start=(info->dlpi_addr+ph->p_vaddr)&~(uintptr_t)4095;
        off_t offset=ph->p_offset&~(ElfW(Off))4095;
        size_t lead=info->dlpi_addr+ph->p_vaddr-start;
        if((ph->p_offset&4095)!=lead)fail("segment alignment");
        size_t length=(lead+ph->p_filesz+4095)&~(size_t)4095,n=length/4096;
        if(!n||n>8191||indices[0]!=n||(uint64_t)ls.st_size!=(n+1)*8||
           offset<0||(uint64_t)offset+length>(uint64_t)ss.st_size)fail("RX extent");
        unsigned char *expected=malloc(length);
        if(!expected || pread(source,expected,length,offset)!=(ssize_t)length)fail("image read");
        if(memcmp((void*)start,expected,length))fail("loaded RX differs from its own file");
        for(size_t j=0;j<n;j++){
            uint64_t index=indices[j+1];
            if(index>=(uint64_t)bs.st_size/4096)fail("page bounds");
            for(size_t k=0;k<j;k++)if(indices[k+1]==index)fail("duplicate page");
            void *want=(void*)(start+j*4096);
            if(mmap(want,4096,PROT_READ|PROT_EXEC,MAP_PRIVATE|MAP_FIXED,backing,(off_t)index*4096)!=want)fail("page map");
        }
        if(memcmp((void*)start,expected,length))fail("mapped RX differs from active variant");
        free(expected);done++;
        ready[1]=start;ready[2]=length;ready[3]=n;
    }
    free(indices);close(source);close(backing);close(layout);
    return 1;
}
__attribute__((constructor))static void placement(void){
    dl_iterate_phdr(visit,NULL);
    if(done!=1)fail("expected one RX segment");
    if(write(198,ready,sizeof(ready))!=(ssize_t)sizeof(ready))fail("ready pipe");
    close(198);
    char go;
    if(read(199,&go,1)!=1||go!='G')fail("release pipe");
    close(199);
}
