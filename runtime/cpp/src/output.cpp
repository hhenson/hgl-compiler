#include <hglrt/output.h>

#include <cstdio>

#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

namespace hglrt {
namespace {

std::FILE* output_file(Stream stream) {
#ifdef _WIN32
    // The Windows CRT otherwise translates LF in text-mode output.
    if (stream == Stream::err) {
        static const int binary = _setmode(_fileno(stderr), _O_BINARY);
        (void)binary;
        return stderr;
    }
    static const int binary = _setmode(_fileno(stdout), _O_BINARY);
    (void)binary;
#endif
    return stream == Stream::err ? stderr : stdout;
}

} // namespace

void fn_write(const str& text, Stream stream, bool flush) {
    std::FILE* output = output_file(stream);
    std::fwrite(text.data(), 1, text.size(), output);
    if (flush) {
        std::fflush(output);
    }
}

} // namespace hglrt
