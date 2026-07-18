@echo off
rem ============================================================================
rem  ShadowScale playtest launcher
rem
rem  This game is two programs: a simulation SERVER and a game CLIENT that talks
rem  to it over local network ports (127.0.0.1). This script starts the server,
rem  waits a moment for it to bind its ports, then launches the client. Closing
rem  the client also shuts the server down.
rem
rem  Just double-click this file. Nothing to install.
rem ============================================================================

setlocal
cd /d "%~dp0"

if not exist "server.exe" (
  echo Could not find server.exe - is the package unzipped fully?
  pause
  exit /b 1
)
if not exist "ShadowScaleClient.exe" (
  echo Could not find ShadowScaleClient.exe - is the package unzipped fully?
  pause
  exit /b 1
)

echo Starting ShadowScale server...
rem Launch the server minimized and capture ITS PID, so shutdown kills exactly
rem this process and not every process named server.exe on the machine.
set "SERVER_PID="
for /f "usebackq" %%p in (`powershell -NoProfile -Command "(Start-Process -FilePath 'server.exe' -WindowStyle Minimized -PassThru).Id"`) do set "SERVER_PID=%%p"

rem Give the server a couple of seconds to bind 127.0.0.1:41000-41003.
timeout /t 2 /nobreak >nul

echo Starting ShadowScale client...
ShadowScaleClient.exe

rem Client closed - stop the server we launched (by PID).
echo Shutting down server...
if defined SERVER_PID (
  taskkill /PID %SERVER_PID% /F >nul 2>&1
) else (
  rem Fallback only if the PID could not be captured.
  taskkill /IM server.exe /F >nul 2>&1
)

endlocal
