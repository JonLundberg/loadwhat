#include <windows.h>
#include <stdio.h>
#include <vector>

int wmain(int argc, wchar_t* argv[]) {
    DWORD buffer_len = GetCurrentDirectoryW(0, nullptr);
    if (buffer_len == 0) {
        wprintf(L"HOST_CWD: <error:%lu>\n", GetLastError());
        return 1;
    }

    std::vector<wchar_t> cwd(buffer_len);
    DWORD written = GetCurrentDirectoryW(buffer_len, cwd.data());
    if (written == 0 || written >= buffer_len) {
        wprintf(L"HOST_CWD: <error:%lu>\n", GetLastError());
        return 1;
    }

    wprintf(L"HOST_CWD: %ls\n", cwd.data());
    wprintf(L"HOST_ARGC: %d\n", argc - 1);
    for (int i = 1; i < argc; ++i) {
        wprintf(L"HOST_ARG[%d]: %ls\n", i - 1, argv[i]);
    }

    return 0;
}
