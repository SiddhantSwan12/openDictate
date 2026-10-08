@echo off
rem Builds the public installer: Whisper on any GPU through Vulkan, with DirectML and the C++ runtime bundled.
rem Needs Visual Studio 2022 Build Tools, the Vulkan SDK and Ninja.
setlocal
for /f "usebackq delims=" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -property installationPath`) do set "VSDIR=%%i"
rem ggml's Vulkan shader generator is a separate CMake project that needs cl.exe on PATH.
call "%VSDIR%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
if not defined VULKAN_SDK (
  for /d %%d in ("C:\VulkanSDK\*") do set "VULKAN_SDK=%%d"
)
if not defined VULKAN_SDK (
  echo Install the Vulkan SDK from https://vulkan.lunarg.com/ first.
  exit /b 1
)
set "PATH=%VULKAN_SDK%\Bin;%PATH%"
rem Ninja builds the shader generator with the cl.exe from this environment (the Visual Studio generator can't).
where ninja >nul 2>nul || (echo Install Ninja: winget install Ninja-build.Ninja & exit /b 1)
set "CMAKE_GENERATOR=Ninja"
rem The shader generator nests deep CMake folders; a short build path keeps them under 260 characters.
if not defined CARGO_TARGET_DIR set "CARGO_TARGET_DIR=%SystemDrive%\odt"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0prepare-redist.ps1" || exit /b 1
call npx tauri build --features vulkan --config src-tauri/tauri.release.conf.json || exit /b 1
echo Installer written to %CARGO_TARGET_DIR%\release\bundle\nsis
