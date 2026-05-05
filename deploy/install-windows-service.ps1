# RavenFabric Agent — Windows Service Installation
# Run as Administrator in PowerShell

# Option 1: Using sc.exe (built-in)
# Note: rf-agent must be compiled with Windows service support
# For now, use NSSM (Non-Sucking Service Manager) as a wrapper

param(
    [string]$BinaryPath = "C:\Program Files\RavenFabric\rf-agent.exe",
    [string]$ConfigPath = "C:\ProgramData\RavenFabric\raven.toml",
    [string]$ServiceName = "RavenFabricAgent"
)

# Create directories
New-Item -ItemType Directory -Force -Path "C:\Program Files\RavenFabric"
New-Item -ItemType Directory -Force -Path "C:\ProgramData\RavenFabric"
New-Item -ItemType Directory -Force -Path "C:\ProgramData\RavenFabric\logs"

# Install using sc.exe (requires the binary to implement Windows Service API)
# For standalone binary without service API, use NSSM below
Write-Host "Installing RavenFabric Agent service..."

# Using sc.exe with the binary directly
sc.exe create $ServiceName `
    binPath= "`"$BinaryPath`" --config `"$ConfigPath`"" `
    start= auto `
    DisplayName= "RavenFabric Agent"

sc.exe description $ServiceName "RavenFabric secure remote execution agent"
sc.exe failure $ServiceName reset= 60 actions= restart/5000/restart/10000/restart/30000

Write-Host "Service installed. Start with: sc.exe start $ServiceName"
Write-Host ""
Write-Host "Alternative: Install NSSM from https://nssm.cc and run:"
Write-Host "  nssm install $ServiceName `"$BinaryPath`" --config `"$ConfigPath`""
Write-Host "  nssm set $ServiceName AppDirectory C:\ProgramData\RavenFabric"
Write-Host "  nssm set $ServiceName AppStdout C:\ProgramData\RavenFabric\logs\agent.log"
Write-Host "  nssm set $ServiceName AppStderr C:\ProgramData\RavenFabric\logs\agent.err"
Write-Host "  nssm start $ServiceName"
