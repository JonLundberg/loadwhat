#include <windows.h>
#include <stdio.h>

typedef int(__cdecl *PFN_LWTEST_FIXTURE_ID)();

int wmain(int argc, wchar_t **argv) {
    if (argc < 2) {
        wprintf(L"HOST: usage: <fullpath-to-dll>\n");
        return 2;
    }

    HMODULE h = LoadLibraryW(argv[1]);
    if (!h) {
        wprintf(L"HOST: LoadLibrary(fullpath) failed gle=%lu\n", GetLastError());
        return 10;
    }

    auto p = (PFN_LWTEST_FIXTURE_ID)GetProcAddress(h, "lwtest_fixture_id");
    if (!p) {
        wprintf(L"HOST: GetProcAddress failed gle=%lu\n", GetLastError());
        FreeLibrary(h);
        return 11;
    }

    int id = p();
    wprintf(L"HOST: lwtest_fixture_id=%d\n", id);

    FreeLibrary(h);
    return 0;
}
