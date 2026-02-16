# Gitsave 开发文档

## 自动保存功能设计

### 当前实现

基于时间间隔的自动保存配置，存储在 `gitsave.toml` 中：

```toml
[auto_save]
enabled = false
interval = 300      # 秒
max_count = 10
```

### 待实现：基于变化检测的自动保存

#### 设计背景

当游戏存档发生修改时自动保存，无需等待时间间隔。特别是处理提交回退场景。

#### 核心逻辑

```
1. 检查冷却时间（上次保存 < 10秒则退出）
2. 检查是否有未提交更改（git status --porcelain），没有则退出
3. git add .           # 暂存所有更改
4. git stash push -m "autosave: <timestamp>"
5. 获取 HEAD 哈希和当前分支名
6. 判断 HEAD 是否在当前分支尖端：
   - 在 → 目标分支 = 当前分支
   - 不在 → 创建新分支（autosave-YYYYMMDD-HHMMSS）
7. git switch 目标分支
8. git stash pop
   - 冲突 → git stash drop，退出（等待下次检测）
   - 成功 → 继续第 9 步
9. git add .           # 重新暂存
10. git commit -m "autosave: YYYY-MM-DD HH:MM:SS"
11. 更新冷却时间（记录上次保存时间戳）
```

#### 关键设计决策

| 决策项 | 决定 |
|--------|------|
| 冷却时间 | 10 秒（固定） |
| 冲突处理 | 放弃 stash，下次检测使用最新状态 |
| 分支命名 | `autosave-YYYYMMDD-HHMMSS` |
| 目标分支选择 | 当前分支尖端 → 复用；否则创建新分支 |

#### 边界情况处理

| 场景 | 处理方式 |
|------|----------|
| stash pop 冲突 | drop stash，退出流程 |
| detached HEAD | 创建新 autosave 分支 |
| 无未提交更改 | 直接退出 |

#### 不需要处理的场景

- 不会出现多个 autosave 分支指向同一个 HEAD（autosave 每次都会产生新提交）

#### 待确认问题

1. 10秒冷却时间是否需要可配置？
2. 是否需要 `--force` 选项处理分支名冲突？

#### 相关文件

- `src/manager/mod.rs`: `SaveManager::should_auto_save()`, `SaveManager::update_last_save_time()`
- `src/manager/mod.rs`: `ConfigManager::load_auto_save_config()`, `ConfigManager::save_auto_save_config()`
- `src/main.rs`: `handle_autosave()`
- `src/cli/mod.rs`: `Autosave` 命令定义

#### 使用示例（未来）

```bash
# 基于时间间隔的自动保存
gitsave autosave --enable --interval 60

# 基于变化检测的自动保存（待实现）
gitsave autosave --watch --on-change

# 显示当前配置
gitsave autosave --status
```

### TUI 集成规划

为了避免在 CLI 中引入伪实时守护逻辑，自动保存调度将与未来的 TUI 客户端协作实施：

1. **事件驱动刷新**：TUI 将维护一个异步任务（基于 tokio runtime 或 crossbeam channel），周期性调用 `SaveManager::should_auto_save()` 并在满足条件时触发 `save`，确保 UI 能展示倒计时与最近一次自动保存结果。
2. **统一状态模型**：TUI 需要读取 `.git/gitsave.toml` 中的自动保存配置，并在 UI 中提供启用、禁用、间隔和最大数量的可视化控制，所有设置仍通过 CLI 同步写回配置文件。
3. **安全开关**：在 TUI 中暴露“实时模式”开关，确保在性能受限的机器上可以手动暂停自动保存，或切换回 CLI-only 模式。
4. **可视化反馈**：TUI 的历史视图展示从 `Git2Core::get_history()` 返回的路线信息，可直接标记由自动保存对应的提交，并允许用户一键清理 `_autosave_*` 标签。

在实现 TUI 前，CLI 的 autosave 命令仅负责配置管理；真正的定时触发将由 TUI 统一驱动，避免重复实现轮询逻辑。
