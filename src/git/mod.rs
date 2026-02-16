use crate::core::{
    ChangeStatus, ChangedFile, CompareResult, PendingChange, RouteInfo, SaveEntry, SaveResult,
    SaveStatus,
};
use crate::error::{Result, SaveError};
use crate::state::{ForkStateManager, root_branch_name};
use chrono::{DateTime, Utc};
use git2::{BranchType, Commit, DiffOptions, Oid, Patch, Repository, ResetType};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct Git2Core {
    repo: Repository,
    workdir: PathBuf,
}

impl Git2Core {
    pub fn init(path: &Path) -> Result<Self> {
        let repo = Repository::init(path).map_err(SaveError::Repository)?;
        let workdir = path.to_path_buf();
        Ok(Self { repo, workdir })
    }

    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path).map_err(SaveError::Repository)?;
        let workdir = repo.workdir().unwrap_or(path).to_path_buf();
        Ok(Self { repo, workdir })
    }

    pub fn repo(&self) -> &Repository {
        &self.repo
    }

    pub fn workdir(&self) -> &PathBuf {
        &self.workdir
    }

    pub fn commit(&mut self, message: &str) -> Result<SaveResult> {
        self.ensure_branch_for_detached()?;

        let mut index = self.repo.index().map_err(SaveError::Repository)?;
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .map_err(SaveError::Repository)?;
        let tree_id = index.write_tree().map_err(SaveError::Repository)?;
        index.write().map_err(SaveError::Repository)?;

        let tree = self
            .repo
            .find_tree(tree_id)
            .map_err(SaveError::Repository)?;

        let sig = self.repo.signature().map_err(SaveError::Repository)?;
        let head = self.repo.head().ok();
        let parent_commit = head.and_then(|h| h.peel_to_commit().ok());

        let mut parents: Vec<&Commit> = Vec::new();
        if let Some(ref commit) = parent_commit {
            parents.push(commit);
        }

        let commit_oid = self
            .repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .map_err(SaveError::Repository)?;

        let changed_files = self.get_changed_files_count()?;

        Ok(SaveResult {
            oid: commit_oid.to_string(),
            short_oid: commit_oid.to_string()[..7].to_string(),
            message: message.to_string(),
            changed_files,
            timestamp: Utc::now(),
        })
    }

    pub fn checkout(&mut self, target: &str) -> Result<()> {
        let commit = self.find_commit(target)?;
        let tree = commit.tree().map_err(SaveError::Repository)?;

        // 在重置之前，创建一个临时标签保存当前状态（防止丢失后续提交）
        if let Ok(head) = self.repo.head() {
            if let Ok(head_commit) = head.peel_to_commit() {
                let timestamp = chrono::Local::now().timestamp();
                let temp_tag = format!("_autosave_{}", timestamp);
                let sig = self.repo.signature().map_err(SaveError::Repository)?;
                let _ = self.repo.tag(
                    &temp_tag,
                    &head_commit.into_object(),
                    &sig,
                    "Auto-save before checkout",
                    false,
                );
            }
        }

        // 配置 checkout 选项：移除不在目标提交中的文件
        let mut checkout_opts = git2::build::CheckoutBuilder::new();
        checkout_opts
            .force()
            .remove_untracked(true) // 删除未跟踪的文件
            .remove_ignored(true); // 删除被忽略的文件

        // 先执行 checkout 到目标 tree
        self.repo
            .checkout_tree(&tree.into_object(), Some(&mut checkout_opts))
            .map_err(SaveError::Repository)?;

        // 记录当前分支，后续设置 HEAD 分离状态后将无法获取
        let base_branch = self
            .repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(|s| s.to_string()));

        // 将 HEAD 设置为分离状态，保持原始分支不变
        self.repo
            .set_head_detached(commit.id())
            .map_err(SaveError::Repository)?;

        // 记录需要派生的基础分支，以便下一次保存创建新路线
        let fork_state = ForkStateManager::new(&self.workdir);
        fork_state.set_pending_base(base_branch.map(|b| root_branch_name(&b)));

        Ok(())
    }

    fn ensure_branch_for_detached(&mut self) -> Result<()> {
        let fork_state = ForkStateManager::new(&self.workdir);
        if !self.repo.head_detached().unwrap_or(false) {
            fork_state.set_pending_base(None);
            return Ok(());
        }

        let head = self.repo.head().map_err(SaveError::Repository)?;
        let commit = head.peel_to_commit().map_err(SaveError::Repository)?;

        let base = fork_state
            .take_pending_base()
            .filter(|b| !b.trim().is_empty())
            .unwrap_or_else(|| "main".to_string());
        let base_root = root_branch_name(&base);
        let new_branch = fork_state.next_branch_name(&base_root);

        self.repo
            .branch(&new_branch, &commit, false)
            .map_err(SaveError::Repository)?;
        self.repo
            .set_head(&format!("refs/heads/{}", new_branch))
            .map_err(SaveError::Repository)?;

        let mut checkout_opts = git2::build::CheckoutBuilder::new();
        checkout_opts.force();
        self.repo
            .checkout_head(Some(&mut checkout_opts))
            .map_err(SaveError::Repository)?;

        println!(
            "[INFO] Detached history detected. Created route '{}' from '{}'.",
            new_branch, base_root
        );

        fork_state.set_pending_base(None);
        Ok(())
    }

    pub fn get_history(&self) -> Result<Vec<SaveEntry>> {
        let mut entries = Vec::new();
        let mut seen_oids = HashSet::new();
        let current_route = self
            .get_current_route_name()
            .unwrap_or_else(|_| "unknown".to_string());
        let commit_owner_map = self.build_commit_owner_map()?;

        let mut revwalk = self.repo.revwalk().map_err(SaveError::Repository)?;

        // 推送所有分支的 HEAD，确保能看到所有提交
        let branches = self.repo.branches(None).map_err(SaveError::Repository)?;
        for branch_result in branches {
            if let Ok((branch, _)) = branch_result {
                if let Ok(reference) = branch.get().peel_to_commit() {
                    let _ = revwalk.push(reference.id());
                }
            }
        }

        // 同时推送当前 HEAD
        let _ = revwalk.push_head();

        // 推送所有标签指向的提交（防止回退后后续提交变成孤儿）
        let tag_names = self.repo.tag_names(None).map_err(SaveError::Repository)?;
        for tag_name in tag_names.iter() {
            if let Some(name) = tag_name {
                if let Ok(oid) = self.repo.refname_to_id(&format!("refs/tags/{}", name)) {
                    if let Ok(tag) = self.repo.find_tag(oid) {
                        if let Ok(commit) = tag.target().and_then(|t| t.peel_to_commit()) {
                            let _ = revwalk.push(commit.id());
                        }
                    } else if let Ok(commit) = self.repo.find_commit(oid) {
                        // 轻量级标签直接指向提交
                        let _ = revwalk.push(commit.id());
                    }
                }
            }
        }

        for oid in revwalk {
            let oid = oid.map_err(SaveError::Repository)?;

            // 去重：避免同一个提交出现在多个分支中
            if seen_oids.contains(&oid) {
                continue;
            }
            seen_oids.insert(oid);

            let commit = self.repo.find_commit(oid).map_err(SaveError::Repository)?;

            let message = commit.message().unwrap_or("").to_string();
            let timestamp = DateTime::from_timestamp(commit.time().seconds(), 0)
                .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH);

            let route = commit_owner_map
                .get(&oid)
                .cloned()
                .unwrap_or_else(|| current_route.clone());

            entries.push(SaveEntry {
                id: oid.to_string(),
                short_id: oid.to_string()[..7].to_string(),
                message,
                timestamp,
                route,
                is_current: false,
            });
        }

        Ok(entries)
    }

    pub fn get_status(&self) -> Result<SaveStatus> {
        let current_route = self.get_current_route_name()?;
        let last_save = self.get_last_save()?;

        let mut pending_changes = Vec::new();
        let mut diff_opts = DiffOptions::new();
        diff_opts.include_untracked(true);
        diff_opts.recurse_untracked_dirs(true);

        let head_tree = self.repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        let diff = match head_tree {
            Some(tree) => self
                .repo
                .diff_tree_to_workdir(Some(&tree), Some(&mut diff_opts)),
            None => self.repo.diff_tree_to_workdir(None, Some(&mut diff_opts)),
        };

        let diff = diff.map_err(SaveError::Repository)?;
        let has_changes = diff.deltas().len() > 0;

        for delta in diff.deltas() {
            let path = delta.new_file().path();
            if let Some(p) = path {
                let status = match delta.status() {
                    git2::Delta::Added => ChangeStatus::Added,
                    git2::Delta::Modified => ChangeStatus::Modified,
                    git2::Delta::Deleted => ChangeStatus::Deleted,
                    _ => ChangeStatus::Modified,
                };
                pending_changes.push(PendingChange {
                    path: p.to_string_lossy().to_string(),
                    status,
                });
            }
        }

        Ok(SaveStatus {
            current_route,
            last_save,
            pending_changes,
            has_uncommitted_changes: has_changes,
        })
    }

    pub fn list_routes(&self) -> Result<Vec<RouteInfo>> {
        let mut routes = Vec::new();
        let branches = self.repo.branches(None).map_err(SaveError::Repository)?;

        for branch_result in branches {
            let (branch, branch_type) = branch_result.map_err(SaveError::Repository)?;
            if branch_type == BranchType::Local {
                let name = match branch.name() {
                    Ok(Some(n)) => n.to_string(),
                    _ => "".to_string(),
                };
                let is_current = self
                    .get_current_route_name()
                    .map(|n| n == name)
                    .unwrap_or(false);

                let latest_save = branch.get().peel_to_commit().ok().map(|c| {
                    let oid = c.id();
                    let timestamp = DateTime::from_timestamp(c.time().seconds(), 0)
                        .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH);
                    SaveEntry {
                        id: oid.to_string(),
                        short_id: oid.to_string()[..7].to_string(),
                        message: c.message().unwrap_or("").to_string(),
                        timestamp,
                        route: name.clone(),
                        is_current,
                    }
                });

                routes.push(RouteInfo {
                    name,
                    is_current,
                    latest_save,
                    save_count: 0,
                });
            }
        }

        Ok(routes)
    }

    pub fn create_route(&mut self, name: &str) -> Result<()> {
        let head = self.repo.head().map_err(SaveError::Repository)?;
        let commit = head.peel_to_commit().map_err(SaveError::Repository)?;

        self.repo
            .branch(name, &commit, false)
            .map_err(SaveError::Repository)?;

        Ok(())
    }

    pub fn switch_route(&mut self, name: &str) -> Result<()> {
        let branch = self
            .repo
            .find_branch(name, BranchType::Local)
            .map_err(SaveError::Repository)?;

        let commit = branch
            .get()
            .peel_to_commit()
            .map_err(SaveError::Repository)?;

        self.repo
            .set_head(&format!("refs/heads/{}", name))
            .map_err(SaveError::Repository)?;

        self.repo
            .reset(&commit.into_object(), ResetType::Hard, None)
            .map_err(SaveError::Repository)?;

        Ok(())
    }

    pub fn switch_create_route(&mut self, name: &str) -> Result<()> {
        self.create_route(name)?;
        self.switch_route(name)?;
        Ok(())
    }

    pub fn list_tags(&self) -> Result<Vec<String>> {
        let mut tags = Vec::new();
        let tag_names = self.repo.tag_names(None)?;
        for name in tag_names.iter() {
            if let Some(tag_name) = name {
                tags.push(tag_name.to_string());
            }
        }
        Ok(tags)
    }

    pub fn create_tag(&self, name: &str, message: &str) -> Result<()> {
        let head = self.repo.head().map_err(SaveError::Repository)?;
        let commit = head.peel_to_commit().map_err(SaveError::Repository)?;
        let sig = self.repo.signature().map_err(SaveError::Repository)?;

        self.repo
            .tag(name, &commit.into_object(), &sig, message, false)
            .map_err(SaveError::Repository)?;
        Ok(())
    }

    pub fn delete_route(&mut self, name: &str) -> Result<()> {
        let mut branch = self
            .repo
            .find_branch(name, BranchType::Local)
            .map_err(SaveError::Repository)?;

        branch.delete().map_err(SaveError::Repository)?;
        Ok(())
    }

    pub fn rename_route(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        let mut branch = self
            .repo
            .find_branch(old_name, BranchType::Local)
            .map_err(SaveError::Repository)?;

        let commit = branch
            .get()
            .peel_to_commit()
            .map_err(SaveError::Repository)?;

        // 检查是否是当前分支
        let is_current = self
            .get_current_route_name()
            .map(|n| n == old_name)
            .unwrap_or(false);

        if is_current {
            // 如果是当前分支，先切换到新分支（创建并切换）
            self.repo
                .branch(new_name, &commit, false)
                .map_err(SaveError::Repository)?;
            self.repo
                .set_head(&format!("refs/heads/{}", new_name))
                .map_err(SaveError::Repository)?;
            branch.delete().map_err(SaveError::Repository)?;
        } else {
            // 如果不是当前分支，直接重命名
            branch.delete().map_err(SaveError::Repository)?;
            self.repo
                .branch(new_name, &commit, false)
                .map_err(SaveError::Repository)?;
        }

        Ok(())
    }

    pub fn delete_tag(&self, name: &str) -> Result<()> {
        self.repo.tag_delete(name).map_err(SaveError::Repository)?;
        Ok(())
    }

    fn build_commit_owner_map(&self) -> Result<HashMap<Oid, String>> {
        let mut owners = HashMap::new();

        let branches = self
            .repo
            .branches(Some(BranchType::Local))
            .map_err(SaveError::Repository)?;
        for branch_result in branches {
            let (branch, _) = branch_result.map_err(SaveError::Repository)?;
            let branch_display_name = branch
                .name()
                .ok()
                .flatten()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "unknown".to_string());

            if let Some(reference_name) = branch.get().name() {
                let mut branch_walk = self.repo.revwalk().map_err(SaveError::Repository)?;
                branch_walk
                    .push_ref(reference_name)
                    .map_err(SaveError::Repository)?;

                for oid in branch_walk {
                    let oid = oid.map_err(SaveError::Repository)?;
                    owners
                        .entry(oid)
                        .or_insert_with(|| branch_display_name.clone());
                }
            }
        }

        let tag_names = self.repo.tag_names(None).map_err(SaveError::Repository)?;
        for tag_name in tag_names.iter().flatten() {
            if let Ok(tag_ref) = self.repo.refname_to_id(&format!("refs/tags/{}", tag_name)) {
                // 先尝试解析为带注释的标签
                if let Ok(tag) = self.repo.find_tag(tag_ref) {
                    if let Ok(commit) = tag.target().and_then(|t| t.peel_to_commit()) {
                        owners
                            .entry(commit.id())
                            .or_insert_with(|| format!("tag:{}", tag_name));
                        continue;
                    }
                }

                if let Ok(commit) = self.repo.find_commit(tag_ref) {
                    owners
                        .entry(commit.id())
                        .or_insert_with(|| format!("tag:{}", tag_name));
                }
            }
        }

        Ok(owners)
    }

    pub fn get_tag_commit(&self, tag_name: &str) -> Result<Commit> {
        let tag_names = self.repo.tag_names(None)?;
        let mut tag_oid = None;
        for name in tag_names.iter() {
            if let Some(name) = name {
                if name == tag_name {
                    if let Ok(oid) = self.repo.refname_to_id(&format!("refs/tags/{}", name)) {
                        tag_oid = Some(oid);
                        break;
                    }
                }
            }
        }

        let oid = tag_oid.ok_or_else(|| SaveError::SaveNotFound(tag_name.to_string()))?;
        let tag = self.repo.find_tag(oid).map_err(SaveError::Repository)?;
        let object = tag.into_object();
        let commit = object.peel_to_commit().map_err(SaveError::Repository)?;
        Ok(commit)
    }

    pub fn checkout_by_tag(&mut self, tag_name: &str) -> Result<()> {
        let commit = self.get_tag_commit(tag_name)?;
        let head = self.repo.head().ok();
        let head_oid = head.as_ref().and_then(|h| h.target());

        if head_oid == Some(commit.id()) {
            return Ok(());
        }

        self.repo
            .reset(&commit.into_object(), ResetType::Hard, None)
            .map_err(SaveError::Repository)?;
        Ok(())
    }

    pub fn compare_saves(&self, id1: &str, id2: &str) -> Result<CompareResult> {
        let commit1 = self.find_commit(id1)?;
        let commit2 = self.find_commit(id2)?;

        let oid1 = commit1.id();
        let oid2 = commit2.id();

        let tree1 = commit1.tree().map_err(SaveError::Repository)?;
        let tree2 = commit2.tree().map_err(SaveError::Repository)?;

        let mut diff_opts = DiffOptions::new();
        let diff = self
            .repo
            .diff_tree_to_tree(Some(&tree1), Some(&tree2), Some(&mut diff_opts))
            .map_err(SaveError::Repository)?;

        let mut changed_files = Vec::new();
        let mut additions = 0usize;
        let mut deletions = 0usize;

        for (idx, delta) in diff.deltas().enumerate() {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string());

            let (file_adds, file_dels) = match Patch::from_diff(&diff, idx) {
                Ok(Some(patch)) => {
                    let (_, adds, dels) = patch.line_stats().map_err(SaveError::Repository)?;
                    (adds as usize, dels as usize)
                }
                _ => {
                    let (adds, dels) = match delta.status() {
                        git2::Delta::Added => (1, 0),
                        git2::Delta::Deleted => (0, 1),
                        _ => (1, 1),
                    };
                    (adds, dels)
                }
            };

            additions += file_adds;
            deletions += file_dels;

            if let Some(path) = path {
                changed_files.push(ChangedFile {
                    path,
                    additions: file_adds,
                    deletions: file_dels,
                });
            }
        }

        let owner_map = self.build_commit_owner_map()?;
        let default_route = self
            .get_current_route_name()
            .unwrap_or_else(|_| "unknown".to_string());

        let entry1 = SaveEntry {
            id: oid1.to_string(),
            short_id: oid1.to_string()[..7].to_string(),
            message: commit1.message().unwrap_or("").to_string(),
            timestamp: DateTime::from_timestamp(commit1.time().seconds(), 0)
                .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH),
            route: owner_map
                .get(&oid1)
                .cloned()
                .unwrap_or_else(|| default_route.clone()),
            is_current: false,
        };

        let entry2 = SaveEntry {
            id: oid2.to_string(),
            short_id: oid2.to_string()[..7].to_string(),
            message: commit2.message().unwrap_or("").to_string(),
            timestamp: DateTime::from_timestamp(commit2.time().seconds(), 0)
                .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH),
            route: owner_map
                .get(&oid2)
                .cloned()
                .unwrap_or_else(|| default_route.clone()),
            is_current: false,
        };

        Ok(CompareResult {
            from: entry1,
            to: entry2,
            additions,
            deletions,
            changed_files,
        })
    }

    fn find_commit(&self, target: &str) -> Result<Commit> {
        // 首先尝试直接解析 OID
        if let Ok(oid) = Oid::from_str(target) {
            if let Ok(commit) = self.repo.find_commit(oid) {
                return Ok(commit);
            }
        }

        // 在所有引用中搜索（所有分支、所有提交、所有标签）
        let mut revwalk = self.repo.revwalk().map_err(SaveError::Repository)?;

        // 推送所有分支的 HEAD
        let branches = self.repo.branches(None).map_err(SaveError::Repository)?;
        for branch_result in branches {
            if let Ok((branch, _)) = branch_result {
                if let Ok(reference) = branch.get().peel_to_commit() {
                    let _ = revwalk.push(reference.id());
                }
            }
        }

        // 同时推送当前 HEAD
        let _ = revwalk.push_head();

        // 推送所有标签指向的提交（防止回退后后续提交变成孤儿）
        let tag_names = self.repo.tag_names(None).map_err(SaveError::Repository)?;
        for tag_name in tag_names.iter() {
            if let Some(name) = tag_name {
                if let Ok(oid) = self.repo.refname_to_id(&format!("refs/tags/{}", name)) {
                    if let Ok(tag) = self.repo.find_tag(oid) {
                        if let Ok(commit) = tag.target().and_then(|t| t.peel_to_commit()) {
                            let _ = revwalk.push(commit.id());
                        }
                    } else if let Ok(commit) = self.repo.find_commit(oid) {
                        // 轻量级标签直接指向提交
                        let _ = revwalk.push(commit.id());
                    }
                }
            }
        }

        for oid in revwalk {
            let oid = oid.map_err(SaveError::Repository)?;
            let short_id = oid.to_string()[..7].to_string();
            if short_id == target {
                let commit = self.repo.find_commit(oid).map_err(SaveError::Repository)?;
                return Ok(commit);
            }
        }

        Err(SaveError::SaveNotFound(target.to_string()))
    }

    fn get_current_route_name(&self) -> Result<String> {
        let head = match self.repo.head() {
            Ok(h) => h,
            Err(_) => return Ok("main".to_string()),
        };
        let branch_name = head.shorthand().unwrap_or("main").to_string();
        Ok(branch_name)
    }

    fn get_last_save(&self) -> Result<Option<SaveEntry>> {
        let head = match self.repo.head().ok() {
            Some(h) => h,
            None => return Ok(None),
        };
        let commit = match head.peel_to_commit().ok() {
            Some(c) => c,
            None => return Ok(None),
        };
        let oid = commit.id();
        let timestamp = DateTime::from_timestamp(commit.time().seconds(), 0)
            .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH);

        Ok(Some(SaveEntry {
            id: oid.to_string(),
            short_id: oid.to_string()[..7].to_string(),
            message: commit.message().unwrap_or("").to_string(),
            timestamp,
            route: self.get_current_route_name()?,
            is_current: true,
        }))
    }

    fn get_changed_files_count(&self) -> Result<usize> {
        let head_tree = self.repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        let mut diff_opts = DiffOptions::new();
        let diff = match head_tree {
            Some(tree) => self
                .repo
                .diff_tree_to_workdir(Some(&tree), Some(&mut diff_opts)),
            None => self.repo.diff_tree_to_workdir(None, Some(&mut diff_opts)),
        };

        let diff = diff.map_err(SaveError::Repository)?;
        Ok(diff.deltas().len())
    }
}
