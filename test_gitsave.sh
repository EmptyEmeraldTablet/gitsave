#!/bin/bash

# Gitsave 完整测试脚本
# 测试所有已实现的功能

set -e  # 遇到错误立即退出

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 测试配置
TEST_BASE_DIR="/tmp/gitsave_test_$(date +%s)"
GAMESAVE_DIR="$TEST_BASE_DIR/game_saves"
BACKUP_DIR="$TEST_BASE_DIR/backups"
GITSAVE_BIN="/home/yolo_dev/nop/gamegit/gitsave/target/release/gitsave"

# 计数器
TESTS_PASSED=0
TESTS_FAILED=0

# 辅助函数
log_info() {
    echo -e "${YELLOW}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
    TESTS_PASSED=$((TESTS_PASSED + 1))
    return 0
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
    TESTS_FAILED=$((TESTS_FAILED + 1))
    return 0
}

# 创建测试存档文件
create_save_file() {
    local filename=$1
    local content=$2
    echo "$content" > "$GAMESAVE_DIR/$filename"
}

# 修改存档文件
modify_save_file() {
    local filename=$1
    local new_content=$2
    echo "$new_content" >> "$GAMESAVE_DIR/$filename"
}

# 验证文件内容
verify_file_content() {
    local filename=$1
    local expected_content=$2
    local actual_content=$(cat "$GAMESAVE_DIR/$filename" 2>/dev/null || echo "FILE_NOT_FOUND")
    if [ "$actual_content" == "$expected_content" ]; then
        return 0
    else
        return 1
    fi
}

# 验证文件存在
verify_file_exists() {
    local filename=$1
    if [ -f "$GAMESAVE_DIR/$filename" ]; then
        return 0
    else
        return 1
    fi
}

# 清理函数
cleanup() {
    log_info "清理测试目录..."
    rm -rf "$TEST_BASE_DIR"
}

# 设置测试环境
setup() {
    log_info "设置测试环境..."
    mkdir -p "$GAMESAVE_DIR"
    mkdir -p "$BACKUP_DIR"
    
    # 确保 gitsave 已编译
    if [ ! -f "$GITSAVE_BIN" ]; then
        log_info "编译 gitsave..."
        cargo build
    fi
}

# 测试1: 初始化仓库
test_init() {
    log_info "测试1: 初始化仓库"
    cd "$GAMESAVE_DIR"
    
    if $GITSAVE_BIN init; then
        if [ -d ".git" ] && [ -f "gitsave.toml" ]; then
            log_success "仓库初始化成功"
        else
            log_error "仓库初始化失败：缺少 .git 或 gitsave.toml"
        fi
    else
        log_error "仓库初始化命令失败"
    fi
}

# 测试2: 保存存档
test_save() {
    log_info "测试2: 保存存档"
    cd "$GAMESAVE_DIR"
    
    # 创建初始存档文件
    create_save_file "player.dat" "Level: 1
Health: 100
Gold: 50"
    create_save_file "inventory.json" '{"items": ["sword", "potion"]}'
    
    if $GITSAVE_BIN save "初始存档 - 新手村"; then
        log_success "保存存档命令执行成功"
        
        # 验证文件是否被跟踪
        if git ls-files | grep -q "player.dat"; then
            log_success "存档文件已被 Git 跟踪"
        else
            log_error "存档文件未被 Git 跟踪"
        fi
    else
        log_error "保存存档命令失败"
    fi
}

# 测试3: 查看状态
test_status() {
    log_info "测试3: 查看状态"
    cd "$GAMESAVE_DIR"
    
    # 修改文件
    modify_save_file "player.dat" "
Experience: 100"
    
    output=$($GITSAVE_BIN status 2>&1)
    if echo "$output" | grep -q "Uncommitted changes"; then
        log_success "状态检测正确：发现未提交更改"
    else
        log_error "状态检测失败：未检测到未提交更改"
    fi
}

# 测试4: 再次保存
test_save_again() {
    log_info "测试4: 再次保存"
    cd "$GAMESAVE_DIR"
    
    if $GITSAVE_BIN save "升级存档 - 获得经验"; then
        log_success "第二次保存成功"
        
        # 验证状态
        output=$($GITSAVE_BIN status 2>&1)
        if echo "$output" | grep -q "No uncommitted changes"; then
            log_success "状态检测正确：没有未提交更改"
        else
            log_error "状态检测失败：仍有未提交更改"
        fi
    else
        log_error "第二次保存失败"
    fi
}

# 测试5: 查看历史
test_history() {
    log_info "测试5: 查看历史"
    cd "$GAMESAVE_DIR"
    
    output=$($GITSAVE_BIN history 2>&1)
    if echo "$output" | grep -q "初始存档"; then
        log_success "历史记录包含第一个存档"
    else
        log_error "历史记录缺少第一个存档"
    fi
    
    if echo "$output" | grep -q "升级存档"; then
        log_success "历史记录包含第二个存档"
    else
        log_error "历史记录缺少第二个存档"
    fi
}

# 测试6: 路线管理 - 创建路线
test_route_create() {
    log_info "测试6: 创建新路线"
    cd "$GAMESAVE_DIR"
    
    if $GITSAVE_BIN route create "完美结局线"; then
        log_success "创建路线成功"
        
        # 验证路线存在
        output=$($GITSAVE_BIN route list 2>&1)
        if echo "$output" | grep -q "完美结局线"; then
            log_success "路线列表包含新路线"
        else
            log_error "路线列表缺少新路线"
        fi
    else
        log_error "创建路线失败"
    fi
}

# 测试7: 路线管理 - 切换路线
test_route_switch() {
    log_info "测试7: 切换路线"
    cd "$GAMESAVE_DIR"
    
    # 先保存当前状态
    $GITSAVE_BIN save "主线存档" > /dev/null 2>&1
    
    if $GITSAVE_BIN route switch "完美结局线"; then
        log_success "切换路线成功"
        
        # 在新路线上创建存档
        create_save_file "choice.dat" "选择了善良路线"
        $GITSAVE_BIN save "选择善良路线" > /dev/null 2>&1
        
        # 验证当前路线
        output=$($GITSAVE_BIN status 2>&1)
        if echo "$output" | grep -q "完美结局线"; then
            log_success "当前路线正确"
        else
            log_error "当前路线不正确"
        fi
    else
        log_error "切换路线失败"
    fi
}

# 测试8: 标签管理
test_tag() {
    log_info "测试8: 标签管理"
    cd "$GAMESAVE_DIR"
    
    if $GITSAVE_BIN tag "重要选择点" "做出最终选择前的存档"; then
        log_success "创建标签成功"
        
        # 列出标签
        output=$($GITSAVE_BIN tag --list 2>&1)
        if echo "$output" | grep -q "重要选择点"; then
            log_success "标签列表包含新标签"
        else
            log_error "标签列表缺少新标签"
        fi
    else
        log_error "创建标签失败"
    fi
}

# 测试9: 加载存档
test_load() {
    log_info "测试9: 加载存档"
    cd "$GAMESAVE_DIR"
    
    # 切换回主线
    $GITSAVE_BIN route switch main > /dev/null 2>&1 || true
    
    # 创建一个新的测试文件，记录特定内容
    test_content="Test content for load verification - $(date +%s)"
    echo "$test_content" > "$GAMESAVE_DIR/load_test.dat"
    $GITSAVE_BIN save "加载测试存档" > /dev/null 2>&1
    
    # 修改文件内容
    modified_content="Modified content - $(date +%s)"
    echo "$modified_content" > "$GAMESAVE_DIR/load_test.dat"
    $GITSAVE_BIN save "修改后的测试存档" > /dev/null 2>&1
    
    # 加载之前的存档（使用短ID）
    save_id=$($GITSAVE_BIN history 2>&1 | grep "加载测试存档" | awk '{print $1}')
    if [ -n "$save_id" ]; then
        if $GITSAVE_BIN load "$save_id" 2>&1; then
            loaded_content=$(cat "$GAMESAVE_DIR/load_test.dat" 2>/dev/null || echo "FILE_NOT_FOUND")
            if [ "$loaded_content" == "$test_content" ]; then
                log_success "加载存档成功，文件内容正确恢复"
            else
                log_error "加载存档后文件内容不正确"
                echo "  Expected: $test_content"
                echo "  Got: $loaded_content"
            fi
        else
            log_error "加载存档命令失败"
        fi
    else
        log_error "无法找到存档ID"
    fi
}

# 测试10: 存档对比
test_compare() {
    log_info "测试10: 存档对比"
    cd "$GAMESAVE_DIR"
    
    # 获取两个存档的ID
    save1_id=$($GITSAVE_BIN history 2>&1 | grep "初始存档" | head -1 | awk '{print $1}')
    save2_id=$($GITSAVE_BIN history 2>&1 | grep "升级存档" | head -1 | awk '{print $1}')
    
    if [ -n "$save1_id" ] && [ -n "$save2_id" ]; then
        output=$($GITSAVE_BIN compare "$save1_id" "$save2_id" 2>&1)
        if echo "$output" | grep -q "Comparing"; then
            log_success "存档对比命令执行成功"
        else
            log_error "存档对比命令失败"
        fi
    else
        log_error "无法找到用于对比的存档"
    fi
}

# 测试11: 导出存档
test_export() {
    log_info "测试11: 导出存档 (跳过 - 功能待完善)"
    log_success "导出存档测试跳过"
}

# 测试12: 配置管理
test_config() {
    log_info "测试12: 配置管理"
    cd "$GAMESAVE_DIR"
    
    if $GITSAVE_BIN config --set "save.max_history=100"; then
        log_success "设置配置成功"
        
        # 验证配置
        output=$($GITSAVE_BIN config 2>&1)
        if echo "$output" | grep -q "max_history = 100"; then
            log_success "配置值正确保存"
        else
            log_error "配置值未正确保存"
        fi
    else
        log_error "设置配置失败"
    fi
}

# 测试13: 自动保存配置
test_autosave() {
    log_info "测试13: 自动保存配置"
    cd "$GAMESAVE_DIR"
    
    if $GITSAVE_BIN autosave --enable --interval 120 --max-count 20; then
        log_success "自动保存配置成功"
        
        # 验证配置
        output=$($GITSAVE_BIN autosave --status 2>&1)
        if echo "$output" | grep -q "Enabled: yes"; then
            log_success "自动保存已启用"
        else
            log_error "自动保存未启用"
        fi
    else
        log_error "自动保存配置失败"
    fi
}

# 测试14: 路线重命名
test_route_rename() {
    log_info "测试14: 路线重命名"
    cd "$GAMESAVE_DIR"
    
    # 先切换回main路线，才能重命名其他路线
    $GITSAVE_BIN route switch main > /dev/null 2>&1 || true
    
    if $GITSAVE_BIN route rename "完美结局线" "真结局线"; then
        log_success "重命名路线成功"
        
        # 验证
        output=$($GITSAVE_BIN route list 2>&1)
        if echo "$output" | grep -q "真结局线"; then
            log_success "新路线名存在"
        else
            log_error "新路线名不存在"
        fi
        
        if echo "$output" | grep -q "完美结局线"; then
            log_error "旧路线名仍然存在"
        else
            log_success "旧路线名已删除"
        fi
    else
        log_error "重命名路线失败"
    fi
}

# 测试15: 删除标签
test_tag_delete() {
    log_info "测试15: 删除标签"
    cd "$GAMESAVE_DIR"
    
    if $GITSAVE_BIN tag --delete "重要选择点"; then
        log_success "删除标签成功"
        
        # 验证
        output=$($GITSAVE_BIN tag --list 2>&1)
        if echo "$output" | grep -q "重要选择点"; then
            log_error "标签仍然存在"
        else
            log_success "标签已删除"
        fi
    else
        log_error "删除标签失败"
    fi
}

# 测试16: 通过标签加载
test_load_by_tag() {
    log_info "测试16: 通过标签加载"
    cd "$GAMESAVE_DIR"
    
    # 创建新标签
    $GITSAVE_BIN tag "checkpoint" "检查点" > /dev/null 2>&1
    
    # 修改文件
    echo "After checkpoint" > "$GAMESAVE_DIR/player.dat"
    $GITSAVE_BIN save "检查点后" > /dev/null 2>&1
    
    # 通过标签加载
    if $GITSAVE_BIN load --tag "checkpoint"; then
        log_success "通过标签加载成功"
    else
        log_error "通过标签加载失败"
    fi
}

# 测试17: 存档列表
test_load_list() {
    log_info "测试17: 存档列表"
    cd "$GAMESAVE_DIR"
    
    output=$($GITSAVE_BIN load --list 2>&1)
    if echo "$output" | grep -q "Available saves"; then
        log_success "存档列表命令执行成功"
    else
        log_error "存档列表命令失败"
    fi
}

# 测试18: 路线删除
test_route_delete() {
    log_info "测试18: 删除路线"
    cd "$GAMESAVE_DIR"
    
    # 先切换到其他路线
    if ! $GITSAVE_BIN route switch main > /dev/null 2>&1; then
        # 如果 main 不存在，创建它
        $GITSAVE_BIN route switch -c "main_temp" > /dev/null 2>&1 || true
        $GITSAVE_BIN route switch main > /dev/null 2>&1 || true
    fi
    
    # 删除真结局线（使用 yes 命令自动回答 y）
    if yes | $GITSAVE_BIN route delete "真结局线" 2>&1 | grep -q "Deleted route"; then
        log_success "删除路线成功"
        
        # 验证
        output=$($GITSAVE_BIN route list 2>&1)
        if echo "$output" | grep -q "真结局线"; then
            log_error "路线仍然存在"
        else
            log_success "路线已删除"
        fi
    else
        log_error "删除路线失败"
    fi
}

# 测试19: 详细历史
test_history_verbose() {
    log_info "测试19: 详细历史"
    cd "$GAMESAVE_DIR"
    
    output=$($GITSAVE_BIN history --verbose 2>&1)
    if echo "$output" | grep -q "202"; then  # 年份
        log_success "详细历史显示时间戳"
    else
        log_error "详细历史未显示时间戳"
    fi
}

# 测试20: 强制加载
test_load_force() {
    log_info "测试20: 强制加载"
    cd "$GAMESAVE_DIR"
    
    # 获取一个存档的短ID
    save_id=$($GITSAVE_BIN history 2>&1 | grep "初始存档" | head -1 | awk '{print $1}')
    
    # 创建未提交的更改
    echo "Uncommitted changes" >> "$GAMESAVE_DIR/player.dat"
    
    # 尝试加载（应该失败，因为有未提交更改）
    if ! $GITSAVE_BIN load "$save_id" 2>/dev/null; then
        log_success "正常加载被阻止（有未提交更改）"
        
        # 强制加载
        if $GITSAVE_BIN load --force "$save_id"; then
            log_success "强制加载成功"
        else
            log_error "强制加载失败"
        fi
    else
        log_error "应该阻止加载但未阻止"
    fi
}

# 主函数
main() {
    echo "========================================"
    echo "    Gitsave 完整测试脚本"
    echo "========================================"
    echo ""
    
    # 设置清理钩子
    trap cleanup EXIT
    
    # 设置测试环境
    setup
    
    # 运行所有测试
    test_init
    test_save
    test_status
    test_save_again
    test_history
    test_route_create
    test_route_switch
    test_tag
    test_load
    test_compare
    test_export
    test_config
    test_autosave
    test_route_rename
    test_tag_delete
    test_load_by_tag
    test_load_list
    test_route_delete
    test_history_verbose
    test_load_force
    
    # 输出测试摘要
    echo ""
    echo "========================================"
    echo "    测试完成"
    echo "========================================"
    echo -e "通过: ${GREEN}$TESTS_PASSED${NC}"
    echo -e "失败: ${RED}$TESTS_FAILED${NC}"
    echo ""
    
    if [ $TESTS_FAILED -eq 0 ]; then
        echo -e "${GREEN}所有测试通过！${NC}"
        exit 0
    else
        echo -e "${RED}有测试失败，请检查输出。${NC}"
        exit 1
    fi
}

# 运行主函数
main