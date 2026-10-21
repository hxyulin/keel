#include <string_view>
#include <cstdint>
#include <cassert>
#include <cstdio>
using Callback = void* (*)(void*, std::string_view);
extern "C" void* view_test(void*, std::string_view, Callback);
extern "C" void* view_helper(void* state, std::string_view view) {
    auto words = static_cast<uint64_t*>(state);
    assert(words[0] == view.size());
    assert(words[1] == (view.empty() ? 0 : static_cast<unsigned char>(view[0])));
    words[2] = view.size() * 17;
    return state;
}
static void* callback(void* state, std::string_view view) {
    auto words = static_cast<uint64_t*>(state);
    assert(words[2] == view.size() * 17);
    words[3] = view.empty() ? 0 : static_cast<unsigned char>(view.back());
    return state;
}
int main() {
    const char bytes[] = "aggregate string view";
    for (size_t n = 0; n < sizeof(bytes); ++n) {
        uint64_t words[4] = {};
        assert(view_test(words, std::string_view(bytes, n), callback) == words);
        assert(words[0] == n);
        assert(words[1] == (n ? static_cast<unsigned char>(bytes[0]) : 0));
        assert(words[2] == n * 17);
        assert(words[3] == (n ? static_cast<unsigned char>(bytes[n-1]) : 0));
    }
    std::puts("22 string_view patterns passed");
}
