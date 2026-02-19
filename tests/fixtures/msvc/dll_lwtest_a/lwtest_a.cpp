#include <windows.h>
#include "../shared/lwtest_ids.h"

#ifndef LWTEST_VARIANT
#define LWTEST_VARIANT 1
#endif

extern "C" __declspec(dllimport) int lwtest_b_force_import();

static void lwtest_touch_b() {
    volatile int x = lwtest_b_force_import();
    (void)x;
}

extern "C" __declspec(dllexport) int lwtest_fixture_id() {
    lwtest_touch_b();

#if LWTEST_VARIANT == 1
    return LWTEST_A_V1_ID;
#elif LWTEST_VARIANT == 2
    return LWTEST_A_V2_ID;
#else
    return 1999;
#endif
}

BOOL WINAPI DllMain(HINSTANCE, DWORD, LPVOID) { return TRUE; }
