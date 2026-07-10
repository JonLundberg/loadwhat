#include <windows.h>
#include "../shared/lwtest_ids.h"

#ifdef LWTEST_B_DEPENDS_ON_C
extern "C" __declspec(dllimport) int lwtest_c_force_import();
#endif

extern "C" __declspec(dllexport) int lwtest_fixture_id() {
    return LWTEST_B_ID;
}

extern "C" __declspec(dllexport) int lwtest_b_force_import() {
#ifdef LWTEST_B_DEPENDS_ON_C
    volatile int c = lwtest_c_force_import();
    (void)c;
#endif
    return LWTEST_B_FORCE_IMPORT_ID;
}

BOOL WINAPI DllMain(HINSTANCE, DWORD, LPVOID) { return TRUE; }
