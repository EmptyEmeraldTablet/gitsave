# Gitsave Windows 安装脚本
# 以管理员身份运行 PowerShell 并执行: iwr -useb https://raw.githubusercontent.com/EmptyEmeraldTablet/gitsave/master/install.ps1 | iex

param(
    [string]$Version = "latest",
    [string]$InstallDir = ""
)

# 错误处理
$ErrorActionPreference = "Stop"

# 颜色函数
function Write-Info($msg) { Write-Host "[INFO] $msg" -ForegroundColor Cyan }
function Write-Success($msg) { Write-Host "[SUCCESS] $msg" -ForegroundColor Green }
function Write-Error($msg) { Write-Host "[ERROR] $msg" -ForegroundColor Red; exit 1 }
function Write-Warning($msg) { Write-Host "[WARNING] $msg" -ForegroundColor Yellow }

# 检测架构
function Get-Architecture {
    if ([Environment]::Is64BitOperatingSystem) {
        return "x86_64"
    } else {
        return "x86"
    }
}

# 获取下载 URL
function Get-DownloadUrl($version) {
    $arch = Get-Architecture
    $repo = "EmptyEmeraldTablet/gitsave"
    
    if ($version -eq "latest") {
        return "https://github.com/$repo/releases/latest/download/gitsave-windows-${arch}.exe"
    } else {
        return "https://github.com/$repo/releases/download/$version/gitsave-windows-${arch}.exe"
    }
}

# 主函数
function Main {
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host "    Gitsave Windows 安装程序" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host ""
    
    # 确定安装目录
    if ($InstallDir) {
        $targetDir = $InstallDir
    } elseif ($env:GITSAVE_INSTALL_DIR) {
        $targetDir = $env:GITSAVE_INSTALL_DIR
    } else {
        # 默认安装到用户目录
        $targetDir = "$env:LOCALAPPDATA\Programs\gitsave"
    }
    
    Write-Info "安装目录: $targetDir"
    
    # 创建目录
    if (!(Test-Path $targetDir)) {
        New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
    }
    
    # 下载
    $downloadUrl = Get-DownloadUrl $Version
    $tmpFile = "$env:TEMP\gitsave-$([Guid]::NewGuid()).exe"
    
    Write-Info "下载 Gitsave..."
    Write-Info "URL: $downloadUrl"
    
    try {
        Invoke-WebRequest -Uri $downloadUrl -OutFile $tmpFile -UseBasicParsing
    } catch {
        Write-Error "下载失败: $_"
    }
    
    # 安装
    $targetFile = Join-Path $targetDir "gitsave.exe"
    Write-Info "安装 Gitsave..."
    Move-Item -Path $tmpFile -Destination $targetFile -Force
    
    # 添加到 PATH
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($currentPath -notlike "*$targetDir*") {
        Write-Info "添加到 PATH 环境变量..."
        [Environment]::SetEnvironmentVariable("Path", "$currentPath;$targetDir", "User")
        # 刷新当前会话的 PATH
        $env:Path = [Environment]::GetEnvironmentVariable("Path", "User")
        Write-Success "已添加到 PATH"
    } else {
        Write-Info "PATH 中已包含安装目录"
    }
    
    # 验证
    if (Test-Path $targetFile) {
        Write-Success "Gitsave 安装成功!"
        Write-Info "位置: $targetFile"
        
        try {
            $versionInfo = & $targetFile --version 2>$null
            Write-Info "版本: $versionInfo"
        } catch {
            Write-Info "版本: 未知"
        }
        
        Write-Host ""
        Write-Host "快速开始:" -ForegroundColor Green
        Write-Host "  gitsave init    # 初始化存档仓库"
        Write-Host "  gitsave save `"第一章完成`"  # 保存存档"
        Write-Host ""
        Write-Host "请重新打开 PowerShell 或命令提示符以使用 gitsave" -ForegroundColor Yellow
    } else {
        Write-Error "安装失败"
    }
}

# 显示帮助
if ($args -contains "--help" -or $args -contains "-h") {
    @"
Gitsave Windows 安装脚本

用法:
    .\install.ps1                    # 安装最新版本
    .\install.ps1 -Version v0.1.0    # 安装指定版本
    .\install.ps1 -InstallDir "C:\Tools"  # 安装到指定目录

或使用 Invoke-WebRequest:
    iwr -useb https://raw.githubusercontent.com/EmptyEmeraldTablet/gitsave/master/install.ps1 | iex

环境变量:
    GITSAVE_INSTALL_DIR    指定安装目录
"@
    exit 0
}

Main