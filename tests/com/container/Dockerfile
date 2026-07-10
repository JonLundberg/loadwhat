# escape=`
FROM mcr.microsoft.com/windows/servercore:ltsc2022

ENV LOADWHAT_CONTAINER_TESTS=1 `
    LOADWHAT_CONTAINER_IMAGE=loadwhat-com-tests

WORKDIR C:\loadwhat
COPY . C:\loadwhat\

ENTRYPOINT ["powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", "C:\\loadwhat\\scripts\\run_container_tests.ps1"]
