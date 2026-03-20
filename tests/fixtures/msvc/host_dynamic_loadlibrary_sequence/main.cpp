#include <windows.h>
#include <stdio.h>
#include <wchar.h>

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
        if (wcsncmp(argv[i], L"sleep:", 6) == 0) {
            DWORD sleep_ms = wcstoul(argv[i] + 6, nullptr, 10);
            Sleep(sleep_ms);
            continue;
        }

        const bool optional = wcsncmp(argv[i], L"optional:", 9) == 0;
        const wchar_t *target = optional ? argv[i] + 9 : argv[i];
        HMODULE h = LoadLibraryW(target);
        if (!h) {
            wprintf(
                L"HOST: LoadLibrary(sequence,%d) failed target=%ls optional=%d gle=%lu\n",
                i,
                target,
                optional ? 1 : 0,
                GetLastError());
            if (!optional) {
                saw_failure = 1;
            }
            continue;
        }

        if (loaded_count < (int)(sizeof(loaded) / sizeof(loaded[0]))) {
            loaded[loaded_count++] = h;
        }

        auto p = (PFN_LWTEST_FIXTURE_ID)GetProcAddress(h, "lwtest_fixture_id");
        if (p) {
            wprintf(
                L"HOST: LoadLibrary(sequence,%d) ok target=%ls optional=%d id=%d\n",
                i,
                target,
                optional ? 1 : 0,
                p());
        } else {
            wprintf(
                L"HOST: LoadLibrary(sequence,%d) ok target=%ls optional=%d no_fixture_export\n",
                i,
                target,
                optional ? 1 : 0);
        }
    }

    for (int i = loaded_count - 1; i >= 0; --i) {
        FreeLibrary(loaded[i]);
    }

    return saw_failure ? 10 : 0;
}
