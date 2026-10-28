# Downloads the helper programs the Windows build ships next to JoTV.exe
# (mpv, ffmpeg, ffprobe, yt-dlp) into src-tauri/windows-tools/.
# tauri.windows.conf.json bundles that folder as `bin/`; src-tauri/src/tools.rs
# looks there first. Run before `tauri build` on Windows (CI does).
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$out = Join-Path $PSScriptRoot '..\src-tauri\windows-tools'
$tmp = Join-Path ([IO.Path]::GetTempPath()) 'jotv-tools'
Remove-Item $out, $tmp -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $out, $tmp | Out-Null
$headers = @{ 'User-Agent' = 'jotv-build' }
if ($env:GITHUB_TOKEN) { $headers.Authorization = "Bearer $env:GITHUB_TOKEN" }

# yt-dlp: a single exe; always the newest, since sites change often
Invoke-WebRequest 'https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe' -OutFile "$out\yt-dlp.exe"

# ffmpeg + ffprobe: LGPL shared build (small exes plus the libav DLLs they need)
$zip = "$tmp\ffmpeg.zip"
Invoke-WebRequest 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-lgpl-shared.zip' -OutFile $zip
Expand-Archive $zip "$tmp\ffmpeg"
$bin = Get-ChildItem "$tmp\ffmpeg" -Recurse -Directory -Filter bin | Select-Object -First 1
Copy-Item "$($bin.FullName)\ffmpeg.exe", "$($bin.FullName)\ffprobe.exe" $out
Copy-Item "$($bin.FullName)\*.dll" $out

# mpv: a single static exe from the newest automated build
$release = Invoke-RestMethod 'https://api.github.com/repos/zhongfly/mpv-winbuild/releases/latest' -Headers $headers
$asset = $release.assets | Where-Object { $_.name -match '^mpv-x86_64-\d{8}-git-[0-9a-f]+\.7z$' } | Select-Object -First 1
if (-not $asset) { throw 'No mpv x86_64 build found in the latest release' }
Invoke-WebRequest $asset.browser_download_url -OutFile "$tmp\mpv.7z"
7z x "$tmp\mpv.7z" "-o$tmp\mpv" -y | Out-Null
Copy-Item "$tmp\mpv\mpv.exe" $out

Remove-Item $tmp -Recurse -Force
Get-ChildItem $out | Format-Table Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } }
