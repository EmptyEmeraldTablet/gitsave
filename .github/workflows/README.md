# GitHub Actions Workflow 说明

本项目使用 GitHub Actions 自动化编译和发布流程。

## Workflows

### 1. CI Workflow (`.github/workflows/ci.yml`)

**触发条件：**
- 推送到 `main` 或 `master` 分支
- 创建 Pull Request 到 `main` 或 `master` 分支

**执行的任务：**
- ✅ 代码格式化检查 (`cargo fmt`)
- ✅ Clippy 静态分析 (`cargo clippy`)
- ✅ Linux 版本构建
- ✅ 运行完整测试套件 (59个测试)
- ✅ Windows 版本构建（Artifact）
- ✅ macOS 版本构建（Artifact）

### 2. Release Workflow (`.github/workflows/release.yml`)

**触发条件：**
- 推送标签以 `v` 开头（例如 `v0.1.0`, `v1.0.0`）

**执行的任务：**
- 🏷️ 创建 GitHub Release
- 🐧 构建 Linux x86_64 版本并上传
- 🪟 构建 Windows x86_64 版本并上传
- 🍎 构建 macOS x86_64 版本并上传
- 🍎 构建 macOS ARM64 (Apple Silicon) 版本并上传
- ✅ 运行测试套件

## 如何创建发布

### 方法1：使用 Git 命令行

```bash
# 1. 确保你在 main 分支上，并且代码已提交
git checkout main
git pull origin main

# 2. 创建标签（遵循语义化版本）
git tag -a v0.1.0 -m "Release version 0.1.0"

# 3. 推送标签到 GitHub
git push origin v0.1.0
```

推送标签后，GitHub Actions 会自动：
1. 创建 Release 页面
2. 编译所有平台的二进制文件
3. 上传到 Release 页面

### 方法2：使用 GitHub Web 界面

1. 访问 https://github.com/EmptyEmeraldTablet/gitsave/releases
2. 点击 "Draft a new release"
3. 点击 "Choose a tag" → "Create new tag"
4. 输入标签名（例如 `v0.1.0`）
5. 填写发布标题和描述
6. 点击 "Publish release"

创建 release 后，workflow 会自动编译并上传二进制文件。

## 版本号规范

推荐使用语义化版本（SemVer）：

- `v0.1.0` - 初始版本
- `v0.2.0` - 新功能（向后兼容）
- `v0.2.1` - 修复 bug
- `v1.0.0` - 正式发布

## 发布的文件

每个 release 包含以下文件：

| 文件名 | 平台 | 说明 |
|--------|------|------|
| `gitsave-linux-x86_64` | Linux 64位 | 静态链接，需要 libgit2 |
| `gitsave-windows-x86_64.exe` | Windows 64位 | 独立可执行文件 |
| `gitsave-macos-x86_64` | macOS Intel | 需要 libgit2 |
| `gitsave-macos-arm64` | macOS Apple Silicon | 需要 libgit2 |

## 系统依赖

### Linux
```bash
# Ubuntu/Debian
sudo apt-get install libgit2-1.1

# Fedora
sudo dnf install libgit2
```

### macOS
```bash
brew install libgit2
```

### Windows
无需额外依赖，单文件可执行。

## 手动下载测试

你也可以手动下载 workflow 构建的 artifact：

1. 访问 https://github.com/EmptyEmeraldTablet/gitsave/actions
2. 选择最新的 workflow 运行
3. 在 "Artifacts" 部分下载对应平台的二进制文件

## 故障排除

### Workflow 失败

1. 检查 Actions 日志：https://github.com/EmptyEmeraldTablet/gitsave/actions
2. 常见失败原因：
   - 依赖安装失败（网络问题）
   - 测试失败（代码问题）
   - 权限问题（检查 `GITHUB_TOKEN` 权限）

### 重新运行 workflow

如果 workflow 失败，可以：
1. 修复代码问题
2. 删除失败的 release（如果是 release workflow）
3. 删除并重新创建标签，推送触发新的 workflow

## 安全说明

- `GITHUB_TOKEN` 是自动提供的，无需手动配置
- 二进制文件通过 GitHub 官方 action 上传，安全可靠
- 所有构建在隔离环境中进行