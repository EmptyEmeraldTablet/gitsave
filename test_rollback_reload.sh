#!/bin/bash

# 测试回退后重新加载后续保存的功能
# 复现用户报告的问题

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

TEST_DIR="/tmp/gitsave_rollback_test_$$"
GITSAVE_BIN="/home/yolo_dev/nop/gamegit/gitsave/target/release/gitsave"

echo "========================================"
echo "    回退后重载测试"
echo "========================================"
echo ""

# 清理函数
cleanup() {
    echo "清理测试目录..."
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

# 创建测试目录
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

echo -e "${YELLOW}[INFO]${NC} 在 $TEST_DIR 中运行测试"

# 1. 初始化
echo ""
echo "步骤1: 初始化仓库"
$GITSAVE_BIN init

# 2. 创建第一个存档
echo ""
echo "步骤2: 创建第一个存档"
echo "save1 content" > save1.txt
$GITSAVE_BIN save "第一个存档"

# 获取第一个存档的ID
FIRST_SAVE_ID=$($GITSAVE_BIN history 2>&1 | grep "第一个存档" | head -1 | awk '{print $1}')
echo "第一个存档ID: $FIRST_SAVE_ID"

# 3. 创建第二个存档
echo ""
echo "步骤3: 创建第二个存档（添加新文件）"
echo "save2 content" > save2.txt
$GITSAVE_BIN save "第二个存档"

# 获取第二个存档的ID
SECOND_SAVE_ID=$($GITSAVE_BIN history 2>&1 | grep "第二个存档" | head -1 | awk '{print $1}')
echo "第二个存档ID: $SECOND_SAVE_ID"

# 4. 验证两个存档都存在
echo ""
echo "步骤4: 验证初始状态"
echo "存档列表:"
$GITSAVE_BIN load --list

SAVE_COUNT=$($GITSAVE_BIN load --list 2>&1 | grep -c "存档")
echo "当前存档数量: $SAVE_COUNT"
if [ "$SAVE_COUNT" -ge 2 ]; then
    echo -e "${GREEN}[PASS]${NC} 初始状态: 存在 $SAVE_COUNT 个存档"
else
    echo -e "${RED}[FAIL]${NC} 初始状态: 期望至少2个存档，实际只有 $SAVE_COUNT 个"
    exit 1
fi

# 5. 回退到第一个存档
echo ""
echo "步骤5: 回退到第一个存档"
$GITSAVE_BIN load --force "$FIRST_SAVE_ID"

# 验证回退成功
if [ ! -f "$TEST_DIR/save2.txt" ]; then
    echo -e "${GREEN}[PASS]${NC} 回退成功: save2.txt 已被删除"
else
    echo -e "${RED}[FAIL]${NC} 回退后 save2.txt 仍然存在"
    exit 1
fi

if [ -f "$TEST_DIR/save1.txt" ]; then
    echo -e "${GREEN}[PASS]${NC} save1.txt 仍然存在"
else
    echo -e "${RED}[FAIL]${NC} save1.txt 被意外删除"
    exit 1
fi

# 6. 关键测试: 回退后检查是否还能看到第二个存档
echo ""
echo "步骤6: 关键测试 - 回退后检查存档列表"
echo "存档列表:"
$GITSAVE_BIN load --list

SAVE_COUNT_AFTER=$($GITSAVE_BIN load --list 2>&1 | grep -c "存档")
echo "回退后存档数量: $SAVE_COUNT_AFTER"

if [ "$SAVE_COUNT_AFTER" -ge 2 ]; then
    echo -e "${GREEN}[PASS]${NC} 回退后仍然能看到所有存档 ($SAVE_COUNT_AFTER 个)"
else
    echo -e "${RED}[FAIL]${NC} 回退后存档丢失！期望至少2个，实际只有 $SAVE_COUNT_AFTER 个"
    echo "这是用户报告的问题！"
    exit 1
fi

# 7. 关键测试: 重新加载第二个存档
echo ""
echo "步骤7: 关键测试 - 重新加载第二个存档"
echo "尝试加载第二个存档 (ID: $SECOND_SAVE_ID)..."

if $GITSAVE_BIN load --force "$SECOND_SAVE_ID"; then
    echo -e "${GREEN}[PASS]${NC} 重新加载第二个存档成功"
    
    # 验证文件状态
    if [ -f "$TEST_DIR/save2.txt" ]; then
        echo -e "${GREEN}[PASS]${NC} save2.txt 已恢复"
    else
        echo -e "${RED}[FAIL]${NC} save2.txt 未恢复"
        exit 1
    fi
else
    echo -e "${RED}[FAIL]${NC} 重新加载第二个存档失败！"
    echo "错误信息: Save not found: $SECOND_SAVE_ID"
    echo "这是用户报告的问题！"
    exit 1
fi

echo ""
echo "========================================"
echo -e "${GREEN}所有测试通过！${NC}"
echo "========================================"
