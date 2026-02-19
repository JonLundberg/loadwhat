#include <windows.h>
#include <stdio.h>

typedef int(__cdecl *PFN_LWTEST_FIXTURE_ID)();

int wmain() {
    HMODULE h = LoadLibraryW(L"lwtest_a.dll");
    if (!h) {
        wprintf(L"HOST: LoadLibrary(name) failed gle=%lu\n", GetLastError());
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
