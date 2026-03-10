#include <windows.h>
#include <stdio.h>

typedef int(__cdecl *PFN_LWTEST_A_START)();

int wmain() {
    HMODULE h = LoadLibraryW(L"lwtest_a.dll");
    if (!h) {
        wprintf(L"HOST: LoadLibrary(nested) failed gle=%lu\n", GetLastError());
        return 10;
    }

    auto p = (PFN_LWTEST_A_START)GetProcAddress(h, "lwtest_a_start");
    if (!p) {
        wprintf(L"HOST: GetProcAddress(lwtest_a_start) failed gle=%lu\n", GetLastError());
        FreeLibrary(h);
        return 11;
    }

    int status = p();
    if (status != 0) {
        wprintf(L"HOST: lwtest_a_start failed status=%d\n", status);
    }

    FreeLibrary(h);
    return status;
}
