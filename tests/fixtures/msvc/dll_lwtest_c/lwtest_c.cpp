#include <windows.h>

extern "C" __declspec(dllexport) int lwtest_c_force_import() {
    return 3003;
}

BOOL WINAPI DllMain(HINSTANCE, DWORD, LPVOID) { return TRUE; }
