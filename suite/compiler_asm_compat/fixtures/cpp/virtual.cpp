// Classes, single/multiple inheritance, virtual dispatch, dynamic_cast, RTTI.
extern "C" int printf(const char *, ...);

struct Shape {
    virtual ~Shape() {}
    virtual long area() const = 0;
    virtual const char *name() const { return "shape"; }
};
struct Rect : Shape {
    long w, h;
    Rect(long w, long h) : w(w), h(h) {}
    long area() const override { return w * h; }
    const char *name() const override { return "rect"; }
};
struct Square final : Rect {
    explicit Square(long s) : Rect(s, s) {}
    const char *name() const override { return "square"; }
};
struct Tri : Shape {
    long b, h;
    Tri(long b, long h) : b(b), h(h) {}
    long area() const override { return b * h / 2; }
};
struct Named { virtual ~Named() {} virtual int tag() const { return 11; } };
struct Both : Rect, Named {
    Both() : Rect(3, 9) {}
    int tag() const override { return 42; }
};

static int destroyed = 0;
struct Counted : Tri { using Tri::Tri; ~Counted() override { destroyed++; } };

__attribute__((noinline)) long total(Shape **shapes, int n) {
    long t = 0;
    for (int i = 0; i < n; i++) {
        t = t * 7 + shapes[i]->area();
        if (dynamic_cast<Rect *>(shapes[i])) t += 1000;
        if (auto *nm = dynamic_cast<Named *>(shapes[i])) t += nm->tag();
    }
    return t;
}

int main(int argc, char **) {
    Shape *shapes[] = {new Rect(argc + 1, 5), new Square(4), new Tri(6, argc + 3), new Both, new Counted(10, 10)};
    long t = total(shapes, 5);
    for (Shape *s : shapes) printf("%s ", s->name());
    for (Shape *s : shapes) delete s;
    printf("\nvirtual %ld destroyed=%d\n", t, destroyed);
    return destroyed == 1 ? 0 : 3;
}
