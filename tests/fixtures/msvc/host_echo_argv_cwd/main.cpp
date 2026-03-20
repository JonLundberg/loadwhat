#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <vector>

int wmain(int argc, wchar_t* argv[]) {
    DWORD sleep_ms = 0;
    int forced_exit_code = 0;
    std::vector<wchar_t*> passthrough_args;

    for (int i = 1; i < argc; ++i) {
        if (wcscmp(argv[i], L"--lwtest-sleep-ms") == 0 && i + 1 < argc) {
            sleep_ms = wcstoul(argv[++i], nullptr, 10);
            continue;
        }
        if (wcscmp(argv[i], L"--lwtest-exit-code") == 0 && i + 1 < argc) {
            forced_exit_code = _wtoi(argv[++i]);
            continue;
        }
        passthrough_args.push_back(argv[i]);
    }

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
    wprintf(L"HOST_ARGC: %zu\n", passthrough_args.size());
    for (size_t i = 0; i < passthrough_args.size(); ++i) {
        wprintf(L"HOST_ARG[%zu]: %ls\n", i, passthrough_args[i]);
    }

    fflush(stdout);
    if (sleep_ms != 0) {
        Sleep(sleep_ms);
    }

    return forced_exit_code;
}
