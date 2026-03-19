# Gitsave - 游戏存档 Git 管理工具

[![Rust](https://img.shields.io/badge/Rust-1.70+-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow)](https://opensource.org/licenses/MIT)

Gitsave 是一个专为游戏存档设计的 Git 管理工具，提供简单的保存/回滚流程，并引入“路线”概念来管理分支结局。

## 快速上手

```bash
# 1) 初始化存档仓库
cd /path/to/game/saves
gitsave init

# 2) 保存当前进度
gitsave save "完成第一章"

# 3) 查看历史
gitsave history

# 4) 回滚到历史存档
gitsave load "完成第一章"
```

启动 TUI：

```bash
gitsave tui
```

## 使用要点

- **保存**：`save` 会把当前存档快照记录为一次提交，可带描述信息。
- **回滚**：`load` 支持按短哈希、描述或标签回滚。
- **路线**：每条路线相当于独立分支，适合管理不同剧情/结局（需要手动创建/切换）。
- **标签**：对关键节点打标，便于回滚与查找。

## TUI 概览

TUI 是首选交互方式，适合频繁保存/回滚场景：
- Routes/History/Status/Notifications 四区布局，直观看到当前路线、历史与工作区状态。
- 支持快捷键快速保存、回滚、切换路线。
- 所有危险操作会弹出确认提示。

完整 TUI 说明请见 [docs/TUI.md](docs/TUI.md)。

## 常见场景

如果你需要快速定位玩法路径，或处理“坏结局/误操作/频繁试错”等情景，建议先看用例清单：  
[docs/USE_CASES.md](docs/USE_CASES.md)

## 文档索引

- 安装与环境配置：[docs/INSTALL.md](docs/INSTALL.md)
- 详细命令参考：[docs/COMMANDS.md](docs/COMMANDS.md)
- TUI 交互说明：[docs/TUI.md](docs/TUI.md)
- 测试说明：[docs/TESTING.md](docs/TESTING.md)
- 常见问题与注意事项：[docs/FAQ.md](docs/FAQ.md)
- 常见使用场景：[docs/USE_CASES.md](docs/USE_CASES.md)

## 尚未实现

- 自动保存与自动分支检测暂不实现（仅保留配置与占位说明）
- 导入/导出备份（`export/import` 暂为占位）
