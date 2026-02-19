#include <windows.h>
#include <stdio.h>

extern "C" __declspec(dllimport) int lwtest_fixture_id();

int wmain() {
    int id = lwtest_fixture_id();
    wprintf(L"HOST: lwtest_fixture_id=%d\n", id);
    return 0;
}
