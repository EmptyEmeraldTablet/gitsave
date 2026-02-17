# 测试说明

项目包含自动化测试脚本 `test_gitsave.sh`。

注意：测试脚本必须在项目根目录运行，会在 `/tmp` 下创建隔离环境并自动清理。

```bash
./test_gitsave.sh
```

测试覆盖：
- init/save/load/status/history
- 路线管理与标签管理
- 存档对比、配置与 autosave
- 文件场景（新增/修改/删除/二进制/特殊字符）
- 路线隔离与性能测试
