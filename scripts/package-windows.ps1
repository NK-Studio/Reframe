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
$Out = "dist\Reframe-$Version-windows-x64"
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

# 2) 배포 폴더 구성: Reframe.exe 옆 ffmpeg\ 폴더에 둔다 (앱이 이 위치를 먼저 찾는다)
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force "$Out\ffmpeg" | Out-Null
Copy-Item target\release\Reframe.exe, target\release\reframe-cli.exe $Out
Copy-Item LICENSE, THIRD_PARTY_NOTICES.md $Out
Copy-Item ui\fonts\OFL.txt "$Out\LICENSE-IBMPlexSansKR.txt"
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
# 백신 검사 등으로 방금 복사한 ffmpeg.exe 가 잠시 잠겨 있을 수 있어 몇 번 다시 시도한다
for ($i = 1; ; $i++) {
    try { Compress-Archive -Path $Out -DestinationPath $Zip -ErrorAction Stop; break }
    catch {
        if ($i -ge 5) { throw }
        Write-Host "압축 재시도 ($i): $_"
        if (Test-Path $Zip) { Remove-Item $Zip }
        Start-Sleep -Seconds 10
    }
}
Write-Host "완료: $Zip"

# 4) 설치 프로그램(MSI): WiX v5 를 build\wix 에 받아 쓴다 (.NET SDK 필요)
$Wix = "build\wix\wix.exe"
if (-not (Test-Path $Wix)) {
    dotnet tool install --tool-path build\wix wix --version 5.0.2
    if ($LASTEXITCODE -ne 0) { throw "WiX 설치 실패" }
}
foreach ($Ext in "WixToolset.UI.wixext/5.0.2", "WixToolset.Util.wixext/5.0.2") {
    & $Wix extension add -g $Ext
    if ($LASTEXITCODE -ne 0) { throw "WiX 확장 설치 실패: $Ext" }
}
# 라이선스 동의 화면용 RTF (LICENSE 원문)
$Rtf = "build\LICENSE.rtf"
$Body = (Get-Content LICENSE -Raw).Replace('\', '\\').Replace('{', '\{').Replace('}', '\}') -replace "`r?`n", "\par`n"
"{\rtf1\ansi\deff0{\fonttbl{\f0 Segoe UI;}}\fs18 $Body}" | Set-Content -Encoding ascii $Rtf
$Msi = "dist\Reframe-$Version-windows-x64.msi"
& $Wix build installer\windows\reframe.wxs -arch x64 -culture ko-KR `
    -ext WixToolset.UI.wixext -ext WixToolset.Util.wixext `
    -d Version=$Version -d SourceDir="$Root\$Out" -d LicenseRtf="$Root\$Rtf" -pdbtype none -o $Msi
if ($LASTEXITCODE -ne 0) { throw "MSI 빌드 실패" }
Write-Host "완료: $Msi"
