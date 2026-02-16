#!/bin/bash

# Gitsave 安装脚本
# 支持 Linux, macOS (Intel/Apple Silicon), Windows (via WSL/Git Bash)

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 版本号
VERSION="${1:-latest}"
REPO="EmptyEmeraldTablet/gitsave"

# 检测操作系统
detect_os() {
    case "$(uname -s)" in
        Linux*)     echo "linux";;
        Darwin*)    echo "macos";;
        CYGWIN*|MINGW*|MSYS*) echo "windows";;
        *)          echo "unknown";;
    esac
}

# 检测架构
detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "x86_64";;
        arm64|aarch64) echo "arm64";;
        *)             echo "unknown";;
    esac
}

# 打印信息
info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# 获取下载 URL
get_download_url() {
    local os=$1
    local arch=$2
    local version=$3
    
    if [ "$version" = "latest" ]; then
        echo "https://github.com/${REPO}/releases/latest/download/gitsave-${os}-${arch}"
    else
        echo "https://github.com/${REPO}/releases/download/${version}/gitsave-${os}-${arch}"
    fi
}

# 主安装函数
main() {
    echo "========================================"
    echo "    Gitsave 安装脚本"
    echo "========================================"
    echo ""
    
    OS=$(detect_os)
    ARCH=$(detect_arch)
    
    info "检测到系统: $OS ($ARCH)"
    
    if [ "$OS" = "unknown" ]; then
        error "不支持的操作系统"
    fi
    
    if [ "$ARCH" = "unknown" ]; then
        error "不支持的架构"
    fi
    
    # Windows 特殊处理
    if [ "$OS" = "windows" ]; then
        INSTALL_NAME="gitsave.exe"
    else
        INSTALL_NAME="gitsave"
    fi
    
    # 确定安装目录
    if [ -n "$INSTALL_DIR" ]; then
        # 使用用户指定的目录
        TARGET_DIR="$INSTALL_DIR"
    elif [ -w "/usr/local/bin" ]; then
        # 有权限写入系统目录
        TARGET_DIR="/usr/local/bin"
    elif [ -d "$HOME/.local/bin" ]; then
        # 使用用户本地目录
        TARGET_DIR="$HOME/.local/bin"
    else
        # 创建用户本地目录
        TARGET_DIR="$HOME/.local/bin"
        mkdir -p "$TARGET_DIR"
    fi
    
    info "安装目录: $TARGET_DIR"
    
    # 检查目标目录是否在 PATH 中
    if [[ ":$PATH:" != *":$TARGET_DIR:"* ]]; then
        warning "$TARGET_DIR 不在 PATH 环境变量中"
        info "请添加以下行到您的 shell 配置文件 (~/.bashrc, ~/.zshrc 等):"
        echo "    export PATH=\"$TARGET_DIR:\$PATH\""
    fi
    
    # 下载二进制文件
    DOWNLOAD_URL=$(get_download_url "$OS" "$ARCH" "$VERSION")
    TMP_FILE="/tmp/gitsave-$$"
    
    info "下载 Gitsave..."
    info "URL: $DOWNLOAD_URL"
    
    if command -v curl &> /dev/null; then
        curl -fsSL -o "$TMP_FILE" "$DOWNLOAD_URL" || error "下载失败"
    elif command -v wget &> /dev/null; then
        wget -q -O "$TMP_FILE" "$DOWNLOAD_URL" || error "下载失败"
    else
        error "需要 curl 或 wget 来下载文件"
    fi
    
    # 验证下载
    if [ ! -f "$TMP_FILE" ]; then
        error "下载文件不存在"
    fi
    
    # 安装
    info "安装 Gitsave..."
    mv "$TMP_FILE" "$TARGET_DIR/$INSTALL_NAME"
    chmod +x "$TARGET_DIR/$INSTALL_NAME"
    
    # 验证安装
    if [ -x "$TARGET_DIR/$INSTALL_NAME" ]; then
        success "Gitsave 安装成功!"
        info "版本: $($TARGET_DIR/$INSTALL_NAME --version 2>/dev/null || echo '未知')"
        info "位置: $TARGET_DIR/$INSTALL_NAME"
        
        echo ""
        echo "快速开始:"
        echo "  gitsave init    # 初始化存档仓库"
        echo "  gitsave save \"第一章完成\"  # 保存存档"
        echo ""
        
        if [[ ":$PATH:" != *":$TARGET_DIR:"* ]]; then
            echo "⚠️  重要提示:"
            echo "   请添加以下行到您的 shell 配置文件:"
            echo "   export PATH=\"$TARGET_DIR:\$PATH\""
            echo ""
            echo "   然后运行: source ~/.bashrc (或对应的配置文件)"
        fi
    else
        error "安装失败"
    fi
}

# 显示帮助
show_help() {
    cat << EOF
Gitsave 安装脚本

用法: $0 [选项] [版本]

选项:
    -h, --help          显示帮助信息
    -d, --dir DIR       指定安装目录

参数:
    版本                要安装的版本号 (默认: latest)
                        例如: v0.1.0

环境变量:
    INSTALL_DIR         安装目录 (优先级高于默认目录)

示例:
    $0                          # 安装最新版本到默认目录
    $0 v0.1.0                   # 安装指定版本
    $0 --dir /usr/local/bin     # 安装到指定目录
    INSTALL_DIR=/opt/bin $0     # 使用环境变量指定目录

EOF
}

# 解析参数
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_help
            exit 0
            ;;
        -d|--dir)
            INSTALL_DIR="$2"
            shift 2
            ;;
        -*)
            error "未知选项: $1"
            ;;
        *)
            VERSION="$1"
            shift
            ;;
    esac
done

main