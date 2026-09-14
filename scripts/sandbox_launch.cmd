@echo off
title YTD Clean Sandbox Tester
echo Disabling Smart App Control if active...
powershell -ExecutionPolicy Bypass -Command "Set-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy' -Name 'VerifiedAndReputablePolicyState' -Value 0 -ErrorAction SilentlyContinue; CiTool.exe -r" >nul 2>&1
echo ========================================================
echo        YTD - Clean Windows Sandbox Environment
echo ========================================================
echo.
echo This is a fresh, isolated Windows machine.
echo Notice that yt-dlp, ffmpeg, and dev tools are NOT installed.
echo.
echo Launching YTD Daemon...
start "" "%~dp0..\target\release\ytd-daemon.exe"

echo Opening Microsoft Edge to Extensions...
start "" "msedge.exe" "edge://extensions"

echo Opening project folder...
explorer.exe "%~dp0.."

echo.
echo Follow the prompts:
echo 1. YTD will prompt with a 1-click dialog to install yt-dlp and ffmpeg.
echo 2. Enable Developer mode in Edge and click 'Load unpacked' -> select 'extension'.
echo.
pause
