# Gitsave - 游戏存档 Git 管理工具

[![Rust](https://img.shields.io/badge/Rust-1.70+-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow)](https://opensource.org/licenses/MIT)

Gitsave 是一个专为游戏存档设计的 Git 管理工具，简化了 Git 的使用流程，让玩家轻松管理游戏进度。

## 特性

### ✅ 已实现
- **简化的工作流**：`init/save/load/status/history` 命令已经稳定可用。
- **路线管理**：完整的 `route list/create/switch/rename/delete` 流程，且支持 TUI 中的快捷切换。
- **标签系统**：`tag` 命令可创建/列出/删除标签，并可通过 `load --tag` 回滚。
- **存档对比**：`compare` 命令输出逐文件增删统计。
- **实验性 TUI**：可视化展示路线、历史、工作区状态，并带有确认提示的快捷操作。
- **自动保存配置 + TUI 轮询**：CLI 可配置 autosave，TUI 会在启用时轮询并执行自动保存。

### 🚧 规划中 / 待完善
- **独立的自动保存守护进程**：目前只有配置和 TUI 轮询，尚未提供后台服务或 CLI 守护。
- **导入/导出备份**：`gitsave export/import` 仍处于占位阶段，后续会实现真正的归档/还原流程。

## 安装

### 方式一：使用安装脚本（推荐）

#### Linux / macOS

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

#### Windows (PowerShell)

```powershell
# 使用 Invoke-WebRequest
iwr -useb https://raw.githubusercontent.com/EmptyEmeraldTablet/gitsave/master/install.ps1 | iex
```

或者手动下载安装：
1. 从 [Releases](https://github.com/EmptyEmeraldTablet/gitsave/releases) 下载 `gitsave-windows-x86_64.exe`
2. 重命名为 `gitsave.exe`
3. 将文件放到 `C:\Users\你的用户名\AppData\Local\Programs\gitsave\`
4. 将该目录添加到系统 PATH 环境变量

### 方式二：手动下载

从 [Releases](https://github.com/EmptyEmeraldTablet/gitsave/releases) 下载对应平台的二进制文件：

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

#### 环境变量配置（手动安装）

安装后需要将 gitsave 所在目录添加到系统 PATH 中，才能在任意位置使用 `gitsave` 命令。

**Linux / macOS:**

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

**Windows:**

> ⚠️ **重要提示**：修改环境变量后，**需要重新打开命令行窗口**才能生效！

1. **图形界面方式（推荐）：**
   - 右键"此电脑" → 属性 → 高级系统设置 → 环境变量
   - 在"用户变量"中找到 `Path`，点击编辑
   - 点击"新建"，添加路径：`C:\Users\你的用户名\AppData\Local\Programs\gitsave`
   - 点击确定保存
   - **重新打开** PowerShell 或命令提示符

2. **PowerShell 方式（以管理员身份运行）：**
   ```powershell
   # 添加环境变量
   [Environment]::SetEnvironmentVariable("Path", $env:Path + ";$env:LOCALAPPDATA\Programs\gitsave", "User")
   
   # 刷新当前会话的 PATH
   $env:Path = [Environment]::GetEnvironmentVariable("Path", "User")
   
   # 验证
   gitsave --version
   ```

3. **命令提示符方式（以管理员身份运行）：**
   ```cmd
   setx PATH "%PATH%;%LOCALAPPDATA%\Programs\gitsave"
   ```
   > ⚠️ 使用 setx 后**必须重新打开**命令提示符才能生效！

**验证安装：**

> ⚠️ **Windows 用户注意**：如果刚安装或修改了 PATH，请**重新打开** PowerShell/命令提示符后再验证！

```bash
# 所有平台通用
gitsave --version

# 应该输出：
# gitsave 0.1.2
```

### 方式三：从源码编译

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

#### 系统依赖

**Ubuntu/Debian:**
```bash
sudo apt-get install libgit2-dev pkg-config
```

**Fedora/RHEL:**
```bash
sudo dnf install libgit2-devel pkgconfig
```

**macOS:**
```bash
brew install libgit2
```

**Windows:**
无需额外依赖，静态编译的二进制文件已包含所有必要的库。

## 测试

项目包含完整的自动化测试脚本 `test_gitsave.sh`，包含 65 个测试用例。

**⚠️ 重要：测试脚本必须在非项目目录运行，会自动在 `/tmp` 下创建隔离测试环境。**

```bash
# 正确用法：在项目根目录运行测试脚本
./test_gitsave.sh

# 脚本会自动：
# 1. 在 /tmp 下创建临时测试目录
# 2. 编译 gitsave（如果未编译）
# 3. 运行 65 个自动化测试
# 4. 自动清理测试目录
```

测试覆盖：
- ✅ 基础功能：init, save, load, status, history
- ✅ 路线管理：创建、切换、重命名、删除
- ✅ 标签系统：创建、删除、按标签加载
- ✅ 存档操作：对比、列表、强制加载
- ✅ 配置管理：config, autosave
- ✅ 文件操作：多文件、大文件、特殊字符、空文件、删除文件、二进制文件
- ✅ 路线隔离：路线切换后文件隔离、标签跨路线共享
- ✅ 特殊字符：存档消息、路线名称
- ✅ 性能测试：快速连续存档、大量存档

## 快速开始

```bash
# 1. 初始化存档仓库
cd /path/to/game/saves
gitsave init

# 2. 保存当前进度
gitsave save "完成第一章"

# 3. 查看历史
gitsave history

# 4. 加载历史存档
gitsave load "第一章"
```

## 命令参考

### init - 初始化存档仓库

在指定目录初始化新的 gitsave 仓库。

```bash
gitsave init [PATH]
```

| 参数 | 说明 |
|------|------|
| `PATH` | 存档目录路径，默认为当前目录 |

**示例:**

```bash
# 初始化当前目录
gitsave init

# 初始化指定目录
gitsave init ./game_saves
```

**输出:**

```
[OK] Initialized gitsave repository
  Location: /path/to/game/saves
  Git path: /path/to/game/saves/.git/
```

---

### save - 保存存档

保存当前游戏状态。

```bash
gitsave save [OPTIONS] [MESSAGE]
```

| 参数 | 说明 |
|------|------|
| `MESSAGE` | 存档描述信息 |

| 选项 | 说明 |
|------|------|
| `-m, --message <MESSAGE>` | 使用命令行消息 |
| `--save-dir <DIR>` | 指定存档目录 |

**示例:**

```bash
# 使用自动时间戳保存
gitsave save

# 指定描述
gitsave save "击败第一个Boss"

# 使用 -m 标志
gitsave save -m "获得强力装备"
```

**输出:**

```
[OK] Save successful!
  ID: a1b2c3d
  Message: 击败第一个Boss
  Files changed: 3
```

---

### load - 加载存档

加载指定的历史存档。

```bash
gitsave load [OPTIONS] [IDENTIFIER]
```

| 参数 | 说明 |
|------|------|
| `IDENTIFIER` | 存档标识（哈希、描述、标签） |

| 选项 | 说明 |
|------|------|
| `-l, --list` | 列出所有可用存档 |
| `-p, --preview` | 预览模式，不实际回退 |
| `-f, --force` | 强制回退，丢弃未保存更改 |
| `-t, --tag <TAG>` | 通过标签加载存档 |

**示例:**

```bash
# 列出所有存档
gitsave load --list

# 使用描述加载
gitsave load "击败Boss"

# 使用短哈希加载
gitsave load a1b2c3d

# 预览加载（不实际回退）
gitsave load --preview "重要选择"

# 通过标签加载
gitsave load --tag "最终存档"

# 强制加载（丢弃未保存更改）
gitsave load --force a1b2c3d
```

**输出 (--list):**

```
Available saves:
  a1b2c3d  - 击败第一个Boss  (current)
  c3d4e5f  - 获得神器
  e5f6g7h  - 重要选择
```

---

### status - 查看状态

查看当前存档状态。

```bash
gitsave status
```

**示例:**

```bash
gitsave status
```

**输出:**

```
Status:
  Current route: main
  Last save: a1b2c3d - 击败Boss  2小时前
  Uncommitted changes: 3 files
    + new_save.dat
    ~ settings.ini
    - old_backup.dat
```

---

### history - 查看历史

查看存档历史记录。

```bash
gitsave history [OPTIONS]
```

| 选项 | 说明 |
|------|------|
| `-v, --verbose` | 显示详细时间信息 |
| `-r, --route <ROUTE>` | 筛选特定路线的历史 |

**示例:**

```bash
# 基本历史
gitsave history

# 详细历史（显示时间戳）
gitsave history --verbose

# 筛选特定路线
gitsave history --route="完美结局线"
```

**输出:**

```
a1b2c3d  * 击败Boss                    2小时前
c3d4e5f    获得神器                      5小时前
e5f6g7h    重要选择                      1天前
g7h8i9j    初始存档                      2天前
```

---

### compare - 比较存档

比较两个存档的差异。

```bash
gitsave compare <SAVE1> <SAVE2>
```

| 参数 | 说明 |
|------|------|
| `SAVE1` | 第一个存档标识 |
| `SAVE2` | 第二个存档标识 |

**示例:**

```bash
# 比较两个存档
gitsave compare "击败Boss" "获得神器"

# 使用哈希比较
gitsave compare a1b2c3d c3d4e5f
```

**输出:**

```
Comparing a1b2c3d and c3d4e5f
Additions: 5, Deletions: 2
  equipment.json: +3 -1
  inventory.bin: +2 -1
```

---

### route - 路线管理

管理游戏路线（类似 Git 分支）。

```bash
gitsave route [COMMAND]
```

| 子命令 | 说明 |
|--------|------|
| `list` | 列出所有路线 |
| `create <NAME>` | 创建新路线 |
| `switch <NAME>` | 切换到指定路线 |
| `switch -c <NAME>` | 创建并切换到新路线 |
| `delete <NAME>` | 删除路线 |
| `rename <OLD> <NEW>` | 重命名路线 |

#### route list - 列出路线

```bash
gitsave route list
```

**示例输出:**

```
Routes:
  main (current) - 击败Boss  2小时前
  完美结局线  - 选择天使路线  3天前
  恶魔路线    - 击败最终Boss  1周前
```

#### route create - 创建路线

```bash
gitsave route create <NAME>
```

**示例:**

```bash
# 创建新路线
gitsave route create "完美结局线"
```

**输出:**

```
[OK] Created route: 完美结局线
```

#### route switch - 切换路线

```bash
gitsave route switch <NAME>
```

**示例:**

```bash
# 切换到已有路线
gitsave route switch "完美结局线"
```

**输出:**

```
[OK] Switched to route: 完美结局线
```

#### route switch -c - 创建并切换

```bash
gitsave route switch -c <NAME>
```

**示例:**

```bash
# 从当前路线创建并切换到新路线
gitsave route switch -c "恶魔路线"
```

**输出:**

```
[OK] Created and switched to route: 恶魔路线
```

#### route delete - 删除路线

```bash
gitsave route delete <NAME>
```

**示例:**

```bash
gitsave route delete "失败存档"
```

**输出:**

```
[OK] Deleted route: 失败存档
```

**注意:** 无法删除当前路线，需要先切换到其他路线。

#### route rename - 重命名路线

```bash
gitsave route rename <OLD_NAME> <NEW_NAME>
```

**示例:**

```bash
gitsave route rename "old-name" "new-name"
```

**输出:**

```
[OK] Renamed route: old-name -> new-name
```

---

### tag - 标签管理

标记重要存档点。

```bash
gitsave tag [OPTIONS] [NAME] [MESSAGE]
```

| 选项 | 说明 |
|------|------|
| `-l, --list` | 列出所有标签 |
| `-d, --delete` | 删除标签 |

**示例:**

```bash
# 创建标签
gitsave tag "最终存档" "打最终Boss前的准备"

# 列出标签
gitsave tag --list

# 删除标签
gitsave tag --delete "最终存档"
```

**输出 (--list):**

```
Tags:
  最终存档  - 打最终Boss前的准备
  v1.0      - 游戏发布版本
```

---

### export - 导出存档

> ⚠️ **注意**：该命令当前仍在实现中，尚未真正打包整个仓库，仅为占位接口。

导出整个存档仓库（规划中）。

```bash
gitsave export <PATH>
```

| 参数 | 说明 |
|------|------|
| `PATH` | 目标文件路径 |

**示例:**

```bash
# 导出到文件
gitsave export ./my_save_backup.gsf

# 导出到目录
gitsave export /path/to/backup/
```

---

### import - 导入存档

> ⚠️ **注意**：该命令当前仍在实现中，仅创建空仓库；未来版本会支持真正的备份还原。

从备份导入存档仓库（规划中）。

```bash
gitsave import <PATH>
```

| 参数 | 说明 |
|------|------|
| `PATH` | 源文件或目录路径 |

**示例:**

```bash
# 从文件导入
gitsave import ./my_save_backup.gsf

# 从目录导入
gitsave import /path/to/backup/
```

---

### config - 配置管理

查看或设置配置。

```bash
gitsave config [set <KEY>=<VALUE>]
```

**示例:**

```bash
# 查看当前配置
gitsave config

# 设置配置项
gitsave config set save.max_history=100
gitsave config set auto_save.enabled=true
```

**输出:**

```
Configuration:
[save]
max_history = 50
compression = 6

[auto_save]
enabled = false
```

---

### autosave - 自动保存配置

配置自动保存功能。目前自动保存仅由 `gitsave tui` 的轮询逻辑触发；CLl 只负责配置，不会后台运行守护进程。

```bash
gitsave autosave [OPTIONS]
```

| 选项 | 说明 |
|------|------|
| `--enable` | 启用自动保存 |
| `--disable` | 禁用自动保存 |
| `--interval <SECONDS>` | 设置保存间隔（秒，最小 60） |
| `--max-count <COUNT>` | 设置最大保存数量（1-100） |
| `--status` | 显示当前配置 |

**示例:**

```bash
# 查看当前配置
gitsave autosave --status

# 启用自动保存（默认设置）
gitsave autosave --enable

# 启用并设置间隔为 60 秒
gitsave autosave --enable --interval 60

# 启用并设置间隔 5 分钟，保留 20 个自动保存
gitsave autosave --enable --interval 300 --max-count 20

# 禁用自动保存
gitsave autosave --disable
```

**输出 (--status):**

```
Auto-save configuration:
  Enabled: yes
  Interval: 300 seconds
  Max count: 10
  Last auto-save: never
```

---

### tui - 图形化界面（实验性）

启动交互式终端界面，汇总路线、历史、工作区与 autosave 状态。按 `q` 退出，`r` 刷新，`Tab` 切换焦点，方向键或 `j/k` 导航。

```bash
gitsave tui
```



**当前能力亮点：**
- 当 autosave 配置启用时，TUI 每秒轮询 `gitsave autosave` 配置并自动执行保存，保存结果通过底部通知面板提示。
- 顶部状态栏实时展示工作目录、当前路线、距离上次刷新时间等信息，可用 `r` 强制刷新。
- 通知面板显示最近的 save / error 日志，便于排查 autosave 或加载异常。
- 内置快捷操作全部采用确认提示，防止误触导致回滚：

| 快捷键 | 作用 | 说明 |
| --- | --- | --- |
| `Enter` (Routes) | 切换到当前选中的路线 | 若工作区有未提交更改，将先提示可能丢失修改 |
| `Enter` (History) / `l` | 加载当前选中的保存点 | 同样在加载前提示是否丢弃未提交内容 |
| `s` | 快速保存当前工作区 | 保存信息自动使用时间戳说明 |
| `c` | 创建并切换到新路线 | 行内输入名称后，仍需 `[y]/[n]` 确认 |
| `a` | 手动触发一次自动保存检测 | 若配置启用且间隔满足，会立即保存；否则输出提示 |
| `j/k` 或 `↑/↓` | 在当前面板内移动焦点 | Routes 面板移动后不会自动跳回 HEAD，方便浏览 |
| `Tab` | 在 Routes / History 面板之间切换 | |

---

## 配置文件

Gitsave 将配置写入 `.git/gitsave.toml`。这样即使玩家执行“整目录回滚”（将工作目录恢复到早期快照而不是 Git 的工作树重置），配置文件仍然随 `.git` 保存下来，不会因为 `.gitignore` 忽略策略而丢失。除非你明确要迁移仓库，否则不需要手动编辑此文件的位置。

```toml
# .git/gitsave.toml

[save]
max_history = 50
compression = 6

[auto_save]
enabled = false
interval = 300
max_count = 10
```

---

## 工作流程示例

### 多周目游戏管理

```bash
# 初始路线完成游戏
gitsave save "第一周目完成"
gitsave tag "week1-complete" "第一周目通关"

# 创建第二周目路线
gitsave route switch -c "第二周目"

# 进行不同选择
gitsave save "选择了恶魔路线"

# 查看两个路线的差异
gitsave compare "week1-complete" "第二周目"
```

### 重要决策点标记

```bash
# 在重要选择前创建标签
gitsave tag "before-final-choice" "最终选择前的存档"

# 做出选择后继续游戏
gitsave save "选择了拯救世界"

# 之后可以随时回滚到决策点
gitsave load --tag "before-final-choice"
```

### 存档备份

```bash
# 定期导出完整存档
gitsave export ./backups/save_$(date +%Y%m%d).gsf

# 从备份恢复
gitsave import ./backups/save_20240115.gsf
```

---

## 路线管理策略

### 线性游戏路线

```
main (主线剧情)
    |
    |-- normal-ending (普通结局)
    |-- good-ending (好结局)
    |-- best-ending (最佳结局)
```

### 分支剧情路线

```
main
    |-- route-a (路线A)
    |   |-- route-a-1 (路线A 变体)
    |-- route-b (路线B)
```

---

## 常见问题

### Q: 误删了存档怎么办？

使用 Git 的 reflog 功能恢复：

```bash
git reflog
git checkout <commit-hash>
```

### Q: 可以和原生 Git 混用吗？

可以，但建议：
- 只在需要高级功能时使用 Git
- 避免直接修改 `.git` 目录
- 使用 `gitsave export` 备份后操作

### Q: 自动保存会占用很多空间吗？

默认保留 10 个自动保存，可以通过 `--max-count` 调整：

```bash
gitsave autosave --enable --max-count 5
```

### Q: 如何迁移到新电脑？

```bash
# 在旧电脑导出
gitsave export ./backup.gsf

# 传输到新电脑后导入
gitsave import ./backup.gsf
```

---

## 高级用法

### 批量操作脚本

```bash
#!/bin/bash
# 自动保存脚本

cd /path/to/game/saves

# 检查是否有未保存更改
if [ -n "$(git status --porcelain)" ]; then
    TIMESTAMP=$(date "+%Y-%m-%d %H:%M:%S")
    gitsave save "自动保存: $TIMESTAMP"
    echo "自动保存完成: $TIMESTAMP"
else
    echo "没有需要保存的更改"
fi
```

### 定时自动保存 (cron)

```bash
# 编辑 crontab
crontab -e

# 添加：每 5 分钟自动保存
*/5 * * * * /path/to/autosave.sh
```

---

## 贡献

欢迎提交 Issue 和 Pull Request！

## 许可证

MIT License
