# 企业技能安装修复说明

## 修复内容

### 问题
企业技能安装成功后，无法在"我的技能"列表中看到新安装的技能。

### 原因
`enterprise_install_skill` 函数只将技能文件复制到中央仓库，但没有将技能记录添加到数据库中。

### 修复方案
在 `src-tauri/src/commands/enterprise.rs` 中修改 `enterprise_install_skill` 函数：

1. **导入 `SkillRecord`** - 添加 `skill_store::{SkillRecord, SkillStore}` 导入

2. **添加数据库插入逻辑** - 在安装成功后，创建 `SkillRecord` 并插入到数据库：
   ```rust
   // Insert skill record to database
   let now = chrono::Utc::now().timestamp_millis();
   let skill_id = uuid::Uuid::new_v4().to_string();
   let central_path_str = result.central_path.to_string_lossy().to_string();

   let record = SkillRecord {
       id: skill_id.clone(),
       name: result.name.clone(),
       description: result.description.clone(),
       source_type: "enterprise".to_string(),
       source_ref: Some(name.clone()),
       source_ref_resolved: None,
       source_subpath: None,
       source_branch: None,
       source_revision: None,
       remote_revision: None,
       central_path: central_path_str,
       content_hash: Some(result.content_hash.clone()),
       enabled: true,
       created_at: now,
       updated_at: now,
       status: "ok".to_string(),
       update_status: "unknown".to_string(),
       last_checked_at: Some(now),
       last_check_error: None,
   };

   store_for_install.insert_skill(&record).map_err(|e| {
       log::error!("[enterprise_install_skill] Failed to insert skill record: {}", e);
       AppError::db(e)
   })?;
   ```

## 测试步骤

1. **启动应用**
   ```bash
   npm run tauri dev
   ```

2. **登录企业账号**
   - 进入设置页面
   - 登录企业账号

3. **安装企业技能**
   - 进入"企业 Skill Market"
   - 选择一个技能（如 `tongtu-pbc`）
   - 点击安装按钮

4. **验证安装结果**
   - 安装成功后，进入"我的技能"页面
   - 应该能看到新安装的企业技能
   - 技能应该显示来源为 "enterprise"

## 构建发布版本

```bash
# 前端构建
npm run build

# 后端构建
cd src-tauri && cargo build --release
```

构建后的可执行文件位于：`src-tauri/target/release/skills-manager.exe`

## 其他已完成的功能

1. ✅ MCP Market 安装功能 - 支持打开提供商控制台
2. ✅ 默认公共 Skill 市场 - 使用 https://skillhub.cn/
3. ✅ Registry MCP "复制 URL" 按钮 - 复制配置到剪贴板
4. ✅ 企业技能安装修复 - 安装后正确显示在"我的技能"中
