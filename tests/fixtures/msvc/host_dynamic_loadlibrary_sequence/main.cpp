#include <windows.h>
#include <stdio.h>

typedef int(__cdecl *PFN_LWTEST_FIXTURE_ID)();

int wmain(int argc, wchar_t **argv) {
    if (argc < 2) {
        wprintf(L"HOST: usage: <dll-path-or-name> [more-dlls...]\n");
        return 2;
    }

    HMODULE loaded[64] = {};
    int loaded_count = 0;
    int saw_failure = 0;

    for (int i = 1; i < argc; ++i) {
        HMODULE h = LoadLibraryW(argv[i]);
        if (!h) {
            wprintf(
                L"HOST: LoadLibrary(sequence,%d) failed target=%ls gle=%lu\n",
                i,
                argv[i],
                GetLastError());
            saw_failure = 1;
            continue;
        }

        if (loaded_count < (int)(sizeof(loaded) / sizeof(loaded[0]))) {
            loaded[loaded_count++] = h;
        }

        auto p = (PFN_LWTEST_FIXTURE_ID)GetProcAddress(h, "lwtest_fixture_id");
        if (p) {
            wprintf(
                L"HOST: LoadLibrary(sequence,%d) ok target=%ls id=%d\n",
                i,
                argv[i],
                p());
        } else {
            wprintf(
                L"HOST: LoadLibrary(sequence,%d) ok target=%ls no_fixture_export\n",
                i,
                argv[i]);
        }
    }

    for (int i = loaded_count - 1; i >= 0; --i) {
        FreeLibrary(loaded[i]);
    }

    return saw_failure ? 10 : 0;
}
