# Runs on the disposable Windows candidate builder after the NSIS install.
$ErrorActionPreference = 'Stop'
$dest = "$env:RUNNER_TEMP\candidate-install"
# Tauri patches the GUI for NSIS, then restores the unpatched target/release copy.
$installers = @(Get-ChildItem target/release/bundle/nsis/*.exe -File)
if ($installers.Count -ne 1) { throw 'Expected exactly one NSIS installer' }
$payload = [IO.Directory]::CreateTempSubdirectory('buzz-nsis-')
try {
  & 7z.exe x -tNSIS -r- -y -bso0 -bsp0 "-o$($payload.FullName)" $installers[0].FullName Buzz.exe
  if ($LASTEXITCODE -ne 0) { throw 'Unable to extract the packaged GUI executable' }
  if ((Get-FileHash "$dest\Buzz.exe").Hash -ne (Get-FileHash "$($payload.FullName)\Buzz.exe").Hash) {
    throw 'Installed executable differs from the NSIS payload'
  }
} finally {
  $payload.Delete($true)
}
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
$listener.Stop()
# WebView2 150+ ignores per-user debugging flags when its host is elevated.
# Use the ordinary user's environment, key store and WebView data folder.
$name = 'buzzsmoke' + [guid]::NewGuid().ToString('N').Substring(0,8)
$password = ConvertTo-SecureString ('T9!' + [guid]::NewGuid().ToString('N')) -AsPlainText -Force
$app = $launcher = $account = $null
try {
  $account = New-LocalUser -Name $name -Password $password -Description 'Disposable Buzz candidate test'
  $sid = $account.SID.Value
  icacls $env:GITHUB_WORKSPACE /deny "*${sid}:(OI)(CI)RX" | Out-Null
  if ($LASTEXITCODE -ne 0) { throw 'Unable to isolate the runner workspace' }
  icacls $dest /grant "*${sid}:(OI)(CI)RX" | Out-Null
  if ($LASTEXITCODE -ne 0) { throw 'Unable to grant read-only access to the installed app' }
  @"
@echo off
whoami /groups | findstr /L /C:"S-1-16-8192" >nul || exit /b 11
if defined ACTIONS_RUNTIME_TOKEN exit /b 12
if defined GITHUB_TOKEN exit /b 12
if defined GITHUB_WORKSPACE exit /b 12
type "$env:GITHUB_WORKSPACE\package.json" >nul 2>&1 && exit /b 13
set "WEBVIEW2_USER_DATA_FOLDER=%LOCALAPPDATA%\BuzzCandidateWebView"
set "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-address=127.0.0.1 --remote-debugging-port=$port --proxy-server=http://127.0.0.1:1"
set "HTTPS_PROXY=http://127.0.0.1:1"
set "HTTP_PROXY=http://127.0.0.1:1"
set "NO_PROXY=localhost,127.0.0.1"
start "" /wait "%~dp0Buzz.exe"
"@ | Set-Content "$dest\startup-control.cmd" -Encoding ascii
  # Leave the environment block unset so Windows builds it from the specified user's profile.
  $start = [Diagnostics.ProcessStartInfo]::new($env:ComSpec, '/d /c startup-control.cmd')
  $start.UseShellExecute = $false
  $start.WorkingDirectory = $dest
  $start.UserName = $account.Name
  $start.Domain = $env:COMPUTERNAME
  $start.Password = $password
  $start.LoadUserProfile = $true
  $launcher = [Diagnostics.Process]::Start($start)
  $deadline = [Diagnostics.Stopwatch]::StartNew()
  do {
    if ($launcher.HasExited) { throw "Standard-user launch failed ($($launcher.ExitCode))" }
    $child = Get-CimInstance Win32_Process -Filter "ParentProcessId=$($launcher.Id) AND Name='Buzz.exe'"
    if ($child) { break }
    Start-Sleep -Milliseconds 250
  } while ($deadline.Elapsed.TotalSeconds -lt 15)
  if (!$child -or $child.ExecutablePath -ne "$dest\Buzz.exe") { throw 'Installed process missing' }
  $owner = Invoke-CimMethod -InputObject $child -MethodName GetOwnerSid
  if ($owner.ReturnValue -ne 0 -or $owner.Sid -ne $sid) { throw 'Unexpected Buzz process owner' }
  $app = Get-Process -Id $child.ProcessId
  Write-Host 'Verified the pinned installed app under the isolated medium-integrity account.'
  $env:NO_PROXY = 'localhost,127.0.0.1'
  $env:BUZZ_TEST_CDP_PORT = $port
  node --test --test-timeout=60000 scripts/verify-windows-registered-hosts.mjs
  $probeExit = $LASTEXITCODE
  if ($app.HasExited) { throw "Installed Buzz exited during IPC ($($app.ExitCode))" }
  $views = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($app.Id) AND Name='msedgewebview2.exe'")
  $configured = @($views | Where-Object { $_.CommandLine -like "*--remote-debugging-port=$port*" })
  if ($probeExit -ne 0 -or $configured.Count -ne 1) {
    throw "Registered-host IPC verification failed (probe=$probeExit, WebViews=$($views.Count), matching debug port=$($configured.Count))"
  }
} finally {
  if ($app -and !$app.HasExited) { $app.Kill($true); $app.WaitForExit() }
  if ($launcher -and !$launcher.HasExited) { $launcher.Kill($true); $launcher.WaitForExit() }
  if ($account) {
    icacls $env:GITHUB_WORKSPACE /remove:d "*$sid" | Out-Null
    Remove-LocalUser -SID $account.SID
  }
  $password.Dispose()
}
