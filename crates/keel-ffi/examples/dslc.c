/* A recursive-descent DSL compiler. All expression values are i64.
 *
 * program  := ("fn" name "(" parameters? ")" "=" expr ";")+
 * expr     := sum
 * sum      := product (("+" | "-") product)*
 * product  := unary ("*" unary)*
 * unary    := "-" unary | primary
 * primary  := integer | parameter | "(" expr ")"
 *           | "if" sum comparison sum "then" expr "else" expr
 * comparison := "==" | "!=" | "<" | "<=" | ">" | ">="
 *
 * Arithmetic wraps modulo 2^64. Comparisons interpret values as signed.
 * No AST is needed: parsing emits Keel IR directly. Branch values meet at a
 * block parameter, so nested conditionals need no mutable variables or phi API.
 */
#include "keel.h"
#include <ctype.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_SOURCE 65536
#define MAX_PARAMS 16
#define MAX_DEPTH 128

enum token_kind {
    END = 256, NAME, NUMBER, FN, IF, THEN, ELSE, EQ, NE, LE, GE
};

typedef struct {
    int kind;
    size_t start, length;
    int64_t number;
} token;

typedef struct {
    token name;
    keel_value value;
} parameter;

typedef struct {
    const char *path;
    char *source;
    size_t position;
    token token;
    unsigned depth;
    keel_module *module;
    keel_function function;
    keel_block current;
    parameter parameters[MAX_PARAMS];
    size_t parameter_count;
} parser;

static void fail(parser *p, const char *message) {
    size_t line = 1, column = 1;
    for (size_t i = 0; i < p->token.start; ++i) {
        if (p->source[i] == '\n') { ++line; column = 1; }
        else ++column;
    }
    fprintf(stderr, "%s:%zu:%zu: %s\n", p->path, line, column, message);
    keel_module_destroy(p->module);
    free(p->source);
    exit(1);
}

static void check(parser *p, keel_status status) {
    if (status != KEEL_OK) fail(p, keel_module_error(p->module));
}

static int word(parser *p, const char *text) {
    return p->token.length == strlen(text)
        && memcmp(p->source + p->token.start, text, p->token.length) == 0;
}

static int same_name(parser *p, token a, token b) {
    return a.length == b.length
        && memcmp(p->source + a.start, p->source + b.start, a.length) == 0;
}

static void next(parser *p) {
    const char *s = p->source;
    for (;;) {
        while (isspace((unsigned char)s[p->position])) ++p->position;
        if (s[p->position] != '#') break;
        while (s[p->position] && s[p->position] != '\n') ++p->position;
    }
    p->token.start = p->position;
    p->token.number = 0;
    unsigned char c = (unsigned char)s[p->position];
    if (!c) { p->token.kind = END; p->token.length = 0; return; }
    ++p->position;
    if (isalpha(c) || c == '_') {
        while (isalnum((unsigned char)s[p->position]) || s[p->position] == '_') ++p->position;
        p->token.kind = NAME;
        p->token.length = p->position - p->token.start;
        if (word(p, "fn")) p->token.kind = FN;
        else if (word(p, "if")) p->token.kind = IF;
        else if (word(p, "then")) p->token.kind = THEN;
        else if (word(p, "else")) p->token.kind = ELSE;
        return;
    }
    if (isdigit(c)) {
        int64_t value = c - '0';
        while (isdigit((unsigned char)s[p->position])) {
            int digit = s[p->position++] - '0';
            if (value > (INT64_MAX - digit) / 10) fail(p, "integer literal exceeds INT64_MAX");
            value = value * 10 + digit;
        }
        p->token.kind = NUMBER;
        p->token.number = value;
    } else {
        p->token.kind = c;
        if (s[p->position] == '=') {
            switch (c) {
                case '=': p->token.kind = EQ; break;
                case '!': p->token.kind = NE; break;
                case '<': p->token.kind = LE; break;
                case '>': p->token.kind = GE; break;
                default: break;
            }
            if (p->token.kind != c) ++p->position;
        }
        if (p->token.kind == c && !strchr("()+-*<>=,;", c)) fail(p, "unexpected character");
    }
    p->token.length = p->position - p->token.start;
}

static void expect(parser *p, int kind, const char *message) {
    if (p->token.kind != kind) fail(p, message);
    next(p);
}

static void enter(parser *p) {
    if (++p->depth > MAX_DEPTH) fail(p, "expression nesting limit exceeded");
}

static keel_value constant(parser *p, int64_t n) {
    keel_value result;
    check(p, keel_build_iconst(p->module, p->current, KEEL_I64, n, &result));
    return result;
}

static keel_value binary(parser *p, keel_binary_op op, keel_value a, keel_value b) {
    keel_value result;
    check(p, keel_build_binary(p->module, p->current, op, a, b, &result));
    return result;
}

static keel_value expression(parser *p);
static keel_value sum(parser *p);

static keel_value conditional(parser *p) {
    next(p); /* if */
    keel_value lhs = sum(p);
    keel_int_cc cc;
    switch (p->token.kind) {
        case EQ: cc = KEEL_EQ; break;
        case NE: cc = KEEL_NE; break;
        case '<': cc = KEEL_SLT; break;
        case LE: cc = KEEL_SLE; break;
        case '>': cc = KEEL_SGT; break;
        case GE: cc = KEEL_SGE; break;
        default: fail(p, "expected comparison in if condition"); return lhs;
    }
    next(p);
    keel_value rhs = sum(p), condition;
    check(p, keel_build_icmp(p->module, p->current, cc, lhs, rhs, &condition));
    expect(p, THEN, "expected 'then'");

    keel_block then_block, else_block, merge;
    check(p, keel_block_create(p->module, p->function, &then_block));
    check(p, keel_block_create(p->module, p->function, &else_block));
    check(p, keel_block_create(p->module, p->function, &merge));
    keel_value result;
    check(p, keel_block_add_parameter(p->module, merge, KEEL_I64, &result));
    check(p, keel_build_brif(p->module, p->current, condition,
                            then_block, NULL, 0, else_block, NULL, 0));
    p->current = then_block;
    keel_value then_value = expression(p);
    check(p, keel_build_jump(p->module, p->current, merge, &then_value, 1));
    expect(p, ELSE, "expected 'else'");
    p->current = else_block;
    keel_value else_value = expression(p);
    check(p, keel_build_jump(p->module, p->current, merge, &else_value, 1));
    p->current = merge;
    return result;
}

static keel_value primary(parser *p) {
    if (p->token.kind == NUMBER) {
        int64_t n = p->token.number;
        next(p);
        return constant(p, n);
    }
    if (p->token.kind == NAME) {
        token name = p->token;
        for (size_t i = 0; i < p->parameter_count; ++i) {
            if (same_name(p, name, p->parameters[i].name)) {
                next(p);
                return p->parameters[i].value;
            }
        }
        fail(p, "unknown parameter");
    }
    if (p->token.kind == '(') {
        next(p);
        keel_value result = expression(p);
        expect(p, ')', "expected ')'");
        return result;
    }
    if (p->token.kind == IF) return conditional(p);
    fail(p, "expected integer, parameter, '(' or 'if'");
    return constant(p, 0); /* unreachable */
}

static keel_value unary(parser *p) {
    enter(p);
    keel_value result;
    if (p->token.kind == '-') {
        next(p);
        keel_value operand = unary(p);
        keel_value zero = constant(p, 0);
        result = binary(p, KEEL_ISUB, zero, operand);
    } else result = primary(p);
    --p->depth;
    return result;
}

static keel_value product(parser *p) {
    keel_value result = unary(p);
    while (p->token.kind == '*') {
        next(p);
        keel_value rhs = unary(p);
        result = binary(p, KEEL_IMUL, result, rhs);
    }
    return result;
}

static keel_value sum(parser *p) {
    keel_value result = product(p);
    while (p->token.kind == '+' || p->token.kind == '-') {
        int op = p->token.kind;
        next(p);
        keel_value rhs = product(p);
        result = binary(p, op == '+' ? KEEL_IADD : KEEL_ISUB, result, rhs);
    }
    return result;
}

static keel_value expression(parser *p) {
    enter(p);
    keel_value result = sum(p);
    --p->depth;
    return result;
}

static void function(parser *p) {
    expect(p, FN, "expected 'fn'");
    if (p->token.kind != NAME) fail(p, "expected function name");
    token name = p->token;
    next(p);
    expect(p, '(', "expected '('");
    p->parameter_count = 0;
    if (p->token.kind != ')') {
        for (;;) {
            if (p->token.kind != NAME) fail(p, "expected parameter name");
            if (p->parameter_count == MAX_PARAMS) fail(p, "too many parameters (maximum 16)");
            for (size_t i = 0; i < p->parameter_count; ++i) {
                if (same_name(p, p->token, p->parameters[i].name)) fail(p, "duplicate parameter");
            }
            p->parameters[p->parameter_count++].name = p->token;
            next(p);
            if (p->token.kind != ',') break;
            next(p);
        }
    }
    expect(p, ')', "expected ')' after parameters");
    expect(p, '=', "expected '=' before function body");
    keel_type types[MAX_PARAMS], returns[] = { KEEL_I64 };
    for (size_t i = 0; i < p->parameter_count; ++i) types[i] = KEEL_I64;
    keel_function f;
    check(p, keel_module_add_function(p->module, (const uint8_t *)p->source + name.start,
                                     name.length, types, p->parameter_count, returns, 1, &f));
    p->function = f;
    check(p, keel_block_create(p->module, f, &p->current));
    for (size_t i = 0; i < p->parameter_count; ++i) {
        check(p, keel_block_add_parameter(p->module, p->current, KEEL_I64, &p->parameters[i].value));
    }
    keel_value result = expression(p);
    expect(p, ';', "expected ';' after function body");
    check(p, keel_build_return(p->module, p->current, &result, 1));
}

static char *read_source(const char *path) {
    FILE *file = fopen(path, "rb");
    if (!file) { perror(path); return NULL; }
    char *source = malloc(MAX_SOURCE + 1);
    if (!source) { fclose(file); return NULL; }
    size_t size = fread(source, 1, MAX_SOURCE + 1, file);
    int valid = !ferror(file) && size <= MAX_SOURCE && !memchr(source, '\0', size);
    if (fclose(file) != 0) valid = 0;
    if (!valid) {
        fprintf(stderr, "%s: unreadable source, embedded NUL, or more than 65536 bytes\n", path);
        free(source);
        return NULL;
    }
    source[size] = '\0';
    return source;
}

int main(int argc, char **argv) {
    if (argc != 4) {
        fprintf(stderr, "usage: %s darwin-arm64|linux-arm64|linux-x86_64 input.dsl output.o\n", argv[0]);
        return 1;
    }
    keel_target target;
    if (strcmp(argv[1], "darwin-arm64") == 0) target = KEEL_AARCH64_APPLE_DARWIN;
    else if (strcmp(argv[1], "linux-arm64") == 0) target = KEEL_AARCH64_LINUX_GNU;
    else if (strcmp(argv[1], "linux-x86_64") == 0) target = KEEL_X86_64_LINUX_GNU;
    else { fprintf(stderr, "unknown target: %s\n", argv[1]); return 1; }
    if (keel_abi_version() != 1) { fprintf(stderr, "incompatible Keel ABI\n"); return 1; }
    parser p = {0};
    p.path = argv[2];
    p.source = read_source(p.path);
    if (!p.source) return 1;
    p.module = keel_module_create();
    next(&p);
    if (p.token.kind == END) fail(&p, "expected at least one function");
    while (p.token.kind != END) function(&p);
    check(&p, keel_module_verify(p.module));
    check(&p, keel_module_optimize(p.module));
    keel_buffer *object = NULL;
    check(&p, keel_module_emit_object(p.module, target, &object));
    keel_module_destroy(p.module);
    free(p.source);
    FILE *file = fopen(argv[3], "wb");
    if (!file) { perror(argv[3]); keel_buffer_destroy(object); return 1; }
    size_t size = keel_buffer_size(object);
    int written = fwrite(keel_buffer_data(object), 1, size, file) == size;
    int closed = fclose(file) == 0;
    keel_buffer_destroy(object);
    if (!written || !closed) { fprintf(stderr, "%s: object write failed\n", argv[3]); return 1; }
    return 0;
}
