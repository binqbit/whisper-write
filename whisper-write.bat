@echo off
color b
cd /d "%~dp0"

if "%1" == "--admin" (
    setlocal EnableDelayedExpansion
    set "args=%*"
    set "args=!args:~8!"
    powershell -Command "Start-Process cmd -ArgumentList '/c \"%~dp0\whisper-write.bat\" !args!' -Verb RunAs"
    endlocal
) else (
    call python ".\src\main.py" %*
)