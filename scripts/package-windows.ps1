# ffmpeg 를 동봉한 Windows 배포본(zip)을 dist/ 에 만든다. (Windows x64, PowerShell)
#
# FFmpeg 는 BtbN/FFmpeg-Builds 의 정적 GPL 빌드를 받아 쓴다.
#   https://github.com/BtbN/FFmpeg-Builds
# 실행: powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
param(
    [string]$FfmpegSeries = "9.0"
)
$ErrorActionPreference = "Stop"

$Root = Resolve-Path "$PSScriptRoot\.."
Set-Location $Root
$Version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
$Out = "dist\Video-Crop-$Version-windows-x64"
$Zip = "$Out.zip"

$env:RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE\.cargo=/cargo --remap-path-prefix=$Root=/src --remap-path-prefix=$env:USERPROFILE=/home"
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build 실패" }

# 1) FFmpeg 받기
$Asset = "ffmpeg-n$FfmpegSeries-latest-win64-gpl-$FfmpegSeries"
$Url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/$Asset.zip"
$Cache = "build\$Asset.zip"
New-Item -ItemType Directory -Force build | Out-Null
if (-not (Test-Path $Cache)) { Invoke-WebRequest $Url -OutFile $Cache }
$Extract = "build\$Asset"
if (Test-Path $Extract) { Remove-Item -Recurse -Force $Extract }
Expand-Archive $Cache -DestinationPath build

# 2) 배포 폴더 구성: video-crop.exe 옆 ffmpeg\ 폴더에 둔다 (앱이 이 위치를 먼저 찾는다)
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force "$Out\ffmpeg" | Out-Null
Copy-Item target\release\video-crop.exe $Out
Copy-Item LICENSE, THIRD_PARTY_NOTICES.md $Out
Copy-Item "$Extract\bin\ffmpeg.exe", "$Extract\bin\ffprobe.exe" "$Out\ffmpeg"
Copy-Item "$Extract\LICENSE.txt" "$Out\ffmpeg\LICENSE-ffmpeg.txt"

# 3) GPL 고지에 필요한 정보 기록 (버전, 빌드 옵션, 소스 위치)
$VersionLine = (& "$Out\ffmpeg\ffmpeg.exe" -hide_banner -version | Select-Object -First 1)
$BuildConf = (& "$Out\ffmpeg\ffmpeg.exe" -hide_banner -buildconf) -join "`n"
@"
$VersionLine (static, Windows x64)
Binary: $Url
Build scripts: https://github.com/BtbN/FFmpeg-Builds
Source: https://git.ffmpeg.org/ffmpeg.git (위 버전 문자열의 커밋)
License: GPL-3.0

$BuildConf
"@ | Set-Content -Encoding utf8 "$Out\ffmpeg\BUILD_INFO.txt"

if (Test-Path $Zip) { Remove-Item $Zip }
Compress-Archive -Path $Out -DestinationPath $Zip
Write-Host "완료: $Zip"
