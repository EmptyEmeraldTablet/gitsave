# 常见问题与注意事项

## 为什么提示不是 gitsave 仓库？
请先在目标存档目录执行 `gitsave init`。

## Windows 上命令找不到？
确认 PATH 已包含安装目录，并重新打开终端窗口。具体步骤见 `docs/INSTALL.md`。

## 为什么切换路线或回滚会提示未保存更改？
切换路线/回滚会覆盖当前工作区。如果存在未保存更改，需要确认是否丢弃。建议先执行 `gitsave save`。

## 游戏失败导致存档变坏，想直接丢弃当前更改怎么做？
在 TUI 的 History 中选择上一条正常存档，按 `Enter` 回滚，确认提示会说明“未保存更改将被丢弃”。这是针对坏结局/误操作后的推荐恢复方式。

## TUI 在 Windows 上卡顿
Windows 终端渲染性能有限，建议使用 Windows Terminal。TUI 已降低重绘频率以减轻卡顿，但大型存档操作仍可能短暂停顿。

## autosave 默认关闭
自动保存需要手动启用：

```bash
gitsave autosave --enable
```

## gitsave.toml 为什么在 .git 内？
为了配合回滚设计，配置文件存储在 `.git` 内部，避免回滚时被误删或被忽略。
