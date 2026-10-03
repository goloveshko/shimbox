@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build.ps1" -TestOnly %*
exit /b %ERRORLEVEL%
