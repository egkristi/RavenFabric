$ErrorActionPreference = 'Stop'

$toolsDir = "$(Split-Path -Parent $MyInvocation.MyCommand.Definition)"
$version  = '0.1.0'
$url64    = "https://github.com/egkristi/RavenFabric-Published/releases/download/v${version}/ravenfabric-x86_64-pc-windows-msvc.zip"

$packageArgs = @{
  packageName    = 'ravenfabric'
  unzipLocation  = $toolsDir
  url64bit       = $url64
  checksum64     = ''
  checksumType64 = 'sha256'
}

Install-ChocolateyZipPackage @packageArgs
