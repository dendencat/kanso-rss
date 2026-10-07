$ErrorActionPreference = 'Stop'
if (-not $env:WINDOWS_CERTIFICATE -or -not $env:WINDOWS_CERTIFICATE_PASSWORD) { throw 'Windows signing certificate configuration is missing' }
$pfxPath = Join-Path $env:RUNNER_TEMP 'kanso-signing.pfx'
[IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
$password = ConvertTo-SecureString $env:WINDOWS_CERTIFICATE_PASSWORD -AsPlainText -Force
$certificate = $null
try {
  $certificate = Import-PfxCertificate -FilePath $pfxPath -CertStoreLocation Cert:\CurrentUser\My -Password $password
  $signTool = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe' | Sort-Object FullName | Select-Object -Last 1
  if (-not $signTool) { throw 'Windows SDK signtool is missing' }
  $files = Get-ChildItem target/release/bundle -Recurse -File | Where-Object { $_.Extension -in '.exe', '.msi' }
  if (-not $files) { throw 'No Windows installers found' }
  foreach ($file in $files) {
    & $signTool.FullName sign /sha1 $certificate.Thumbprint /fd SHA256 /tr https://timestamp.digicert.com /td SHA256 $file.FullName
    if ($LASTEXITCODE -ne 0) { throw "Signing failed for $($file.Name)" }
    & $signTool.FullName verify /pa $file.FullName
    if ($LASTEXITCODE -ne 0) { throw "Signature verification failed for $($file.Name)" }
  }
} finally {
  Remove-Item $pfxPath -Force -ErrorAction SilentlyContinue
  if ($certificate) { Remove-Item "Cert:\CurrentUser\My\$($certificate.Thumbprint)" -ErrorAction SilentlyContinue }
}
