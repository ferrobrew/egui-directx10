# PowerShell script to compile HLSL shaders for DirectX 10
# This script finds fxc.exe and compiles the shaders

$ErrorActionPreference = "Stop"

# Function to find fxc.exe
function Find-FxcCompiler {
    # First, check if fxc is in PATH
    $fxcInPath = Get-Command fxc.exe -ErrorAction SilentlyContinue
    if ($fxcInPath) {
        Write-Host "Found fxc.exe in PATH: $($fxcInPath.Source)"
        return $fxcInPath.Source
    }

    # Search for fxc.exe in Windows SDK
    Write-Host "Searching for fxc.exe in Windows SDK..."
    $windowsKitsPath = "C:\Program Files (x86)\Windows Kits"

    if (Test-Path $windowsKitsPath) {
        # Find all fxc.exe files and prefer x64 version
        $fxcFiles = Get-ChildItem -Path $windowsKitsPath -Recurse -Filter fxc.exe -ErrorAction SilentlyContinue

        # Prefer x64 version
        $fxcX64 = $fxcFiles | Where-Object { $_.DirectoryName -like "*\x64" } | Select-Object -First 1
        if ($fxcX64) {
            Write-Host "Found fxc.exe (x64): $($fxcX64.FullName)"
            return $fxcX64.FullName
        }

        # Fall back to any version
        $fxcAny = $fxcFiles | Select-Object -First 1
        if ($fxcAny) {
            Write-Host "Found fxc.exe: $($fxcAny.FullName)"
            return $fxcAny.FullName
        }
    }

    throw "fxc.exe not found. Please install Windows SDK or add fxc.exe to your PATH."
}

# Main script
try {
    $fxcPath = Find-FxcCompiler

    # Change to shaders directory
    Push-Location shaders

    Write-Host "`nCompiling vertex shader..."
    & $fxcPath egui.hlsl /nologo /O3 /T vs_4_0 /E vs_egui /Fo vs_egui.bin
    if ($LASTEXITCODE -ne 0) {
        throw "Vertex shader compilation failed with exit code $LASTEXITCODE"
    }
    Write-Host "Vertex shader compiled successfully: vs_egui.bin"

    Write-Host "`nCompiling pixel shader..."
    & $fxcPath egui.hlsl /nologo /O3 /T ps_4_0 /E ps_egui /Fo ps_egui.bin
    if ($LASTEXITCODE -ne 0) {
        throw "Pixel shader compilation failed with exit code $LASTEXITCODE"
    }
    Write-Host "Pixel shader compiled successfully: ps_egui.bin"

    Write-Host "`nAll shaders compiled successfully!"

} catch {
    Write-Error $_.Exception.Message
    exit 1
} finally {
    Pop-Location
}
