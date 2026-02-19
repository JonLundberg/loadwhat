#include <windows.h>
#include "../shared/lwtest_ids.h"

extern "C" __declspec(dllexport) int lwtest_fixture_id() {
    return LWTEST_B_ID;
}

extern "C" __declspec(dllexport) int lwtest_b_force_import() {
    return LWTEST_B_FORCE_IMPORT_ID;
}

BOOL WINAPI DllMain(HINSTANCE, DWORD, LPVOID) { return TRUE; }
