# 소스에서 실행할 때 쓸 ffmpeg 를 target\debug, target\release 의 ffmpeg\ 폴더에 넣는다. (Windows x64, PowerShell)
#
# 시스템에 설치된 ffmpeg 가 LGPL 빌드면 x264/x265 가 없어 코덱 목록에서 빠지므로,
# 배포본과 같은 BtbN/FFmpeg-Builds 정적 GPL 빌드를 받아 쓴다. (앱은 이 위치를 PATH 보다 먼저 찾는다)
#   https://github.com/BtbN/FFmpeg-Builds
# 실행: powershell -ExecutionPolicy Bypass -File scripts\fetch-ffmpeg-windows.ps1
param(
    [string]$FfmpegSeries = "9.0"
)
$ErrorActionPreference = "Stop"

$Root = Resolve-Path "$PSScriptRoot\.."
Set-Location $Root

# 1) FFmpeg 받기 (package-windows.ps1 과 같은 build\ 캐시를 쓴다)
$Asset = "ffmpeg-n$FfmpegSeries-latest-win64-gpl-$FfmpegSeries"
$Url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/$Asset.zip"
$Cache = "build\$Asset.zip"
New-Item -ItemType Directory -Force build | Out-Null
if (-not (Test-Path $Cache)) { Invoke-WebRequest $Url -OutFile $Cache }
$Extract = "build\$Asset"
if (Test-Path $Extract) { Remove-Item -Recurse -Force $Extract }
Expand-Archive $Cache -DestinationPath build

# 2) cargo run (debug / --release) 실행 파일 옆 ffmpeg\ 폴더에 복사
foreach ($BuildProfile in "debug", "release") {
    $Dest = "target\$BuildProfile\ffmpeg"
    New-Item -ItemType Directory -Force $Dest | Out-Null
    Copy-Item "$Extract\bin\ffmpeg.exe", "$Extract\bin\ffprobe.exe" $Dest
    Write-Host "복사: $Dest"
}
& "$Extract\bin\ffmpeg.exe" -hide_banner -version | Select-Object -First 1
