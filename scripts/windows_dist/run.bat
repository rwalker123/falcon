@echo off
rem ============================================================================
rem  ShadowScale playtest launcher
rem
rem  This game is two programs: a simulation SERVER and a game CLIENT that talks
rem  to it over local network ports (127.0.0.1). This script starts the server,
rem  waits a moment for it to bind its ports, then launches the client. Closing
rem  the client also shuts the server window down.
rem
rem  Just double-click this file. Nothing to install.
rem ============================================================================

setlocal
cd /d "%~dp0"

echo Starting ShadowScale server...
start "ShadowScale Server" /min server.exe

rem Give the server a couple of seconds to bind 127.0.0.1:41000-41003.
timeout /t 2 /nobreak >nul

echo Starting ShadowScale client...
ShadowScaleClient.exe

rem Client closed — stop the server window too.
echo Shutting down server...
taskkill /IM server.exe /F >nul 2>&1

endlocal
