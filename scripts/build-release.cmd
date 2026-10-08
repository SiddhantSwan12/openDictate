@echo off
rem Builds the public installer inside a Visual Studio developer environment.
rem ggml's Vulkan shader generator is configured as a separate CMake project that needs cl.exe on PATH.
setlocal
for /f "usebackq delims=" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -property installationPath`) do set "VSDIR=%%i"
call "%VSDIR%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
if not defined VULKAN_SDK (
  for /d %%d in ("C:\VulkanSDK\*") do set "VULKAN_SDK=%%d"
)
if not defined VULKAN_SDK (
  echo Install the Vulkan SDK from https://vulkan.lunarg.com/ first.
  exit /b 1
)
set "PATH=%VULKAN_SDK%\Bin;%PATH%"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0prepare-redist.ps1" || exit /b 1
call npx tauri build --features vulkan --config src-tauri/tauri.release.conf.json
