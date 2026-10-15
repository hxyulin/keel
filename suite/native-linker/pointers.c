#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dlfcn.h>
extern int other(int);
int writable = 19;
int zeroes[128];
static const char message[] = "native Mach-O pointers";
static const char *pointer = message + 7;
static int (*external_call)(const char *) = puts;
static int (*internal_call)(int) = other;
int exported_for_dlsym(void) { return writable; }
int main(int argc, char **argv) {
    if (argc != 3 || strcmp(argv[1], "one") || strcmp(argv[2], "two")) return 1;
    if (zeroes[127] || strcmp(pointer, "Mach-O pointers")) return 2;
    if (internal_call(4) != 23 || external_call(message) < 0) return 3;
    int (*found)(void) = (int (*)(void))dlsym(RTLD_DEFAULT, "exported_for_dlsym");
    if (!found || found() != 19) return 4;
    char *p = malloc(8); if (!p) return 5;
    strcpy(p, "heap"); puts(p); free(p);
    return 0;
}
