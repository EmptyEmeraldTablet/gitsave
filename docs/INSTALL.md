# 安装与环境配置

## 方式一：使用安装脚本（推荐）

### Linux / macOS

```bash
# 使用 curl
curl -fsSL https://raw.githubusercontent.com/EmptyEmeraldTablet/gitsave/master/install.sh | bash

# 或者使用 wget
wget -qO- https://raw.githubusercontent.com/EmptyEmeraldTablet/gitsave/master/install.sh | bash
```

安装完成后，如果命令不在 PATH 中，请添加：

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Windows (PowerShell)

```powershell
# 使用 Invoke-WebRequest
iwr -useb https://raw.githubusercontent.com/EmptyEmeraldTablet/gitsave/master/install.ps1 | iex
```

或者手动下载安装：
1. 从 Releases 下载 `gitsave-windows-x86_64.exe`
2. 重命名为 `gitsave.exe`
3. 将文件放到 `C:\Users\你的用户名\AppData\Local\Programs\gitsave\`
4. 将该目录添加到系统 PATH 环境变量

## 方式二：手动下载

从 Releases 下载对应平台的二进制文件：

| 平台 | 文件名 | 安装路径 |
|------|--------|----------|
| Linux x86_64 | `gitsave-linux-x86_64` | `~/.local/bin/gitsave` 或 `/usr/local/bin/gitsave` |
| macOS Intel | `gitsave-macos-x86_64` | `~/.local/bin/gitsave` 或 `/usr/local/bin/gitsave` |
| macOS Apple Silicon | `gitsave-macos-arm64` | `~/.local/bin/gitsave` 或 `/usr/local/bin/gitsave` |
| Windows x86_64 | `gitsave-windows-x86_64.exe` | `C:\Users\用户名\AppData\Local\Programs\gitsave\gitsave.exe` |

安装示例（Linux/macOS）：

```bash
# 下载
wget https://github.com/EmptyEmeraldTablet/gitsave/releases/latest/download/gitsave-linux-x86_64

# 移动到 PATH 目录
chmod +x gitsave-linux-x86_64
mv gitsave-linux-x86_64 ~/.local/bin/gitsave

# 验证安装
gitsave --version
```

## 环境变量配置（手动安装）

安装后需要将 gitsave 所在目录添加到系统 PATH 中，才能在任意位置使用 `gitsave` 命令。

### Linux / macOS

```bash
# 添加到用户配置（推荐）
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc

# 如果使用 zsh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc

# 验证
gitsave --version
```

### Windows

重要提示：修改环境变量后，需要重新打开命令行窗口才能生效。

1. 图形界面方式（推荐）：
   - 右键“此电脑” -> 属性 -> 高级系统设置 -> 环境变量
   - 在“用户变量”中找到 `Path`，点击编辑
   - 点击“新建”，添加路径：`C:\Users\你的用户名\AppData\Local\Programs\gitsave`
   - 点击确定保存
   - 重新打开 PowerShell 或命令提示符

2. PowerShell 方式（以管理员身份运行）：
   ```powershell
   [Environment]::SetEnvironmentVariable("Path", $env:Path + ";$env:LOCALAPPDATA\Programs\gitsave", "User")
   $env:Path = [Environment]::GetEnvironmentVariable("Path", "User")
   gitsave --version
   ```

3. 命令提示符方式（以管理员身份运行）：
   ```cmd
   setx PATH "%PATH%;%LOCALAPPDATA%\Programs\gitsave"
   ```
   使用 setx 后必须重新打开命令提示符。

## 从源码编译

需要 Rust 1.70+ 和 libgit2 开发库：

```bash
# 克隆仓库
git clone https://github.com/EmptyEmeraldTablet/gitsave.git
cd gitsave

# 编译
cargo build --release

# 安装到系统目录
sudo cp target/release/gitsave /usr/local/bin/

# 或安装到用户目录
cp target/release/gitsave ~/.local/bin/
```

### 系统依赖

Ubuntu/Debian:
```bash
sudo apt-get install libgit2-dev pkg-config
```

Fedora/RHEL:
```bash
sudo dnf install libgit2-devel pkgconfig
```

macOS:
```bash
brew install libgit2
```

Windows:
无需额外依赖，静态编译的二进制文件已包含所有必要的库。
