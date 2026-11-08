param(
 [Parameter(Mandatory=$true)][string]$BaseUrl,
 [Parameter(Mandatory=$true)][string]$CAFile,
 [Parameter(Mandatory=$true)][string]$TokenFile
)
$ErrorActionPreference = 'Stop'
$Repo = Split-Path $PSScriptRoot -Parent
$Root = Join-Path $env:LOCALAPPDATA 'PhyXiEnterprise'
New-Item -ItemType Directory -Force $Root | Out-Null
$Account = "$env:USERDOMAIN\$env:USERNAME"
icacls $Root /inheritance:r | Out-Null
icacls $Root /grant:r "${Account}:(OI)(CI)F" 'SYSTEM:(OI)(CI)F' | Out-Null
if (-not (Get-Command py -ErrorAction SilentlyContinue)) {
 $Installer = Join-Path $Root 'python-3.12.10-amd64.exe'
 Invoke-WebRequest 'https://www.python.org/ftp/python/3.12.10/python-3.12.10-amd64.exe' -OutFile $Installer
 $Sig = Get-AuthenticodeSignature $Installer
 if ($Sig.Status -ne 'Valid' -or $Sig.SignerCertificate.Subject -notmatch 'Python Software Foundation') { throw 'Python 签名不符' }
 $Process = Start-Process -Wait -PassThru $Installer -ArgumentList '/quiet InstallAllUsers=0 Include_pip=1 Include_launcher=1 InstallLauncherAllUsers=0 PrependPath=1 Include_test=0'
 if ($Process.ExitCode -notin @(0,3010)) { throw 'Python 安装失败' }
 throw 'Python 安装完成；重新打开 PowerShell（返回 3010 时先重启），重新执行本脚本。'
}
py -3.12 --version
if ($LASTEXITCODE -ne 0) { throw '安装 Python 3.12 后重试' }
$Project = Join-Path $Root 'project-app'
if (Test-Path $Project) { throw 'project-app 已存在；使用既有项目按 docs/workstation.md 接入，避免覆盖' }
$Commit = '020ae85544dd55ebbc82166dbb7d3573aa7baee2'
$Zip = Join-Path $Root 'project.zip'
Invoke-WebRequest "https://github.com/qlsupersky/PhyXiCATIAagent/archive/$Commit.zip" -OutFile $Zip
Expand-Archive $Zip -DestinationPath $Root
Move-Item (Join-Path $Root "PhyXiCATIAagent-$Commit") $Project
py -3.12 -m venv (Join-Path $Root 'venv')
if ($LASTEXITCODE -ne 0) { throw 'venv 创建失败' }
$Python = Join-Path $Root 'venv\Scripts\python.exe'
& $Python -m pip install -r (Join-Path $Project 'requirements.txt') Pillow
if ($LASTEXITCODE -ne 0) { throw '项目依赖安装失败' }
& $Python (Join-Path $PSScriptRoot 'install-adapter.py') --project $Project
if ($LASTEXITCODE -ne 0) { throw '项目连接适配失败' }
$Meta = Invoke-RestMethod 'https://registry.npmjs.org/@opencode%2fcli-windows-x64/2.2.0'
$Archive = Join-Path $Root 'opencode.tgz'
Invoke-WebRequest $Meta.dist.tarball -OutFile $Archive
$Hasher = [Security.Cryptography.SHA512]::Create(); $Stream = [IO.File]::OpenRead($Archive)
try { $Hash = [Convert]::ToBase64String($Hasher.ComputeHash($Stream)) } finally { $Stream.Dispose(); $Hasher.Dispose() }
if (('sha512-'+$Hash) -ne $Meta.dist.integrity) { throw 'OpenCode 校验失败' }
tar -xzf $Archive -C $Root
if ($LASTEXITCODE -ne 0) { throw 'OpenCode 解压失败' }
$Binary = (Get-ChildItem (Join-Path $Root 'package') -Recurse -Filter opencode.exe | Select-Object -First 1).FullName
Copy-Item $CAFile (Join-Path $Root 'company-ca.pem')
Copy-Item $TokenFile (Join-Path $Root 'cad-ws-01.token')
& $Python (Join-Path $PSScriptRoot 'start-client.py') --root $Root --binary $Binary --base-url $BaseUrl --ca (Join-Path $Root 'company-ca.pem') --token-file (Join-Path $Root 'cad-ws-01.token') --model glm-5.3-flash
if ($LASTEXITCODE -ne 0) { throw 'OpenCode 启动失败' }
$env:PHYXI_OPENCODE_CONNECTION = Join-Path $Root 'opencode-gateway\connection.json'
$env:NO_PROXY='localhost,127.0.0.1,::1';$env:no_proxy=$env:NO_PROXY
Set-Location $Project
& $Python web_server.py --workspace (Join-Path $Root 'opencode-gateway\workspace') --project (Join-Path $Root 'opencode-gateway\project') --browser
