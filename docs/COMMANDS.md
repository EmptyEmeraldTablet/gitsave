# 命令参考

本文件列出 gitsave 的完整指令与常用示例。建议在终端里使用 `gitsave --help` 获取最新信息。

## init

初始化存档仓库。

```bash
gitsave init [PATH]
```

选项：
- `-f, --force`：强制在已有 gitsave 仓库中重新初始化（谨慎使用）

示例：

```bash
gitsave init
```

## save

保存当前游戏状态。

```bash
gitsave save [-m MESSAGE] [DESC]
```

- `-m, --message`：指定保存描述
- `DESC`：可选的描述（与 `-m` 二选一即可）

示例：

```bash
gitsave save "击败第一个Boss"
```

## load

加载存档。支持列出、预览、强制回滚或通过标签回滚。

```bash
gitsave load [OPTIONS] [IDENTIFIER]
```

- `-l, --list`：列出所有存档
- `-p, --preview`：预览模式，不实际回滚
- `-f, --force`：强制回滚（丢弃未保存更改）
- `-t, --tag TAG`：通过标签加载
- `IDENTIFIER`：短哈希或存档描述

示例：

```bash
gitsave load --list
gitsave load a1b2c3d
gitsave load "重要选择"
gitsave load --tag "最终存档"
```

## status

查看当前仓库状态。

```bash
gitsave status
```

## history

查看存档历史。

```bash
gitsave history [OPTIONS]
```

- `-v, --verbose`：显示详细时间信息
- `-r, --route ROUTE`：筛选特定路线的历史

## compare

比较两个存档之间的差异。

```bash
gitsave compare <SAVE1> <SAVE2>
```

## route

路线管理（相当于游戏分支）。

```bash
gitsave route [OPTIONS] [COMMAND]
```

- `-l, --list`：列出路线

子命令：

```bash
gitsave route list
gitsave route create <NAME>
gitsave route switch <NAME>
gitsave route switch -c <NAME>   # 创建并切换
gitsave route delete <NAME>
gitsave route rename <OLD> <NEW>
```

## tag

对关键存档打标。

```bash
gitsave tag [OPTIONS] [NAME] [MESSAGE]
```

- `-l, --list`：列出标签
- `-d, --delete`：删除标签

示例：

```bash
gitsave tag "boss-1" "击败Boss"
gitsave tag --list
gitsave tag --delete "boss-1"
```

## export / import

导出/导入仓库（目前为占位，功能待完善）。

```bash
gitsave export <PATH>
gitsave import <PATH>
```

## config

查看或设置配置。

```bash
gitsave config [--set KEY=VALUE]
```

## autosave

自动保存配置（默认关闭）。

```bash
gitsave autosave [OPTIONS]
```

- `--enable`：启用
- `--disable`：禁用
- `--interval SECONDS`：设置间隔（>= 60 秒）
- `--max_count COUNT`：设置最大保存数量（1-100）
- `--status`：查看当前配置

## tui

启动 TUI：

```bash
gitsave tui
```

TUI 交互说明详见 `docs/TUI.md`。
