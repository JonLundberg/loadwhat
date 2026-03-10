#include <windows.h>
#include "../shared/lwtest_ids.h"

#ifndef LWTEST_VARIANT
#define LWTEST_VARIANT 1
#endif

#if LWTEST_VARIANT != 4
extern "C" __declspec(dllimport) int lwtest_b_force_import();
#endif

static void lwtest_touch_b() {
#if LWTEST_VARIANT != 4
    volatile int x = lwtest_b_force_import();
    (void)x;
#endif
}

extern "C" __declspec(dllexport) int lwtest_a_start() {
#if LWTEST_VARIANT == 4
    HMODULE h = LoadLibraryW(L"lwtest_b.dll");
    if (!h) {
        return 10;
    }
    FreeLibrary(h);
#endif
    return 0;
}

extern "C" __declspec(dllexport) int lwtest_fixture_id() {
    lwtest_touch_b();

#if LWTEST_VARIANT == 1
    return LWTEST_A_V1_ID;
#elif LWTEST_VARIANT == 2
    return LWTEST_A_V2_ID;
#elif LWTEST_VARIANT == 3
    return 3001;
#elif LWTEST_VARIANT == 4
    return LWTEST_A_NESTED_ID;
#else
    return 1999;
#endif
}

BOOL WINAPI DllMain(HINSTANCE, DWORD, LPVOID) {
#if LWTEST_VARIANT == 3
    return FALSE;
#else
    return TRUE;
#endif
}
