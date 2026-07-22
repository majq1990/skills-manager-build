# Skill Manager 使用说明

## 一、简介

Skill Manager 是企业级 Skill 分发系统的客户端工具，用于从服务器下载、安装和管理 Skill（技能包）。

### 主要功能
- 用户认证与权限管理
- Skill 列表查看
- Skill 下载安装
- 批量同步 Skill
- 支持部门专属 Skill 访问

---

## 二、安装与配置

### 2.1 文件位置
win:

https://demo.egova.com.cn/MediaRoot/gc/skill-manager/win/skill-manager.exe

mac:

https://demo.egova.com.cn/MediaRoot/gc/skill-manager/mac/skill-manager.sh



### 2.2 配置文件
首次运行会自动创建配置文件：
- 配置路径: %APPDATA%\skill-sync\config.yaml
- Token 存储: %USERPROFILE%\.config\skill-sync\token.enc
- 用户信息: %USERPROFILE%\.config\skill-sync\user_info.json
- Skill 安装路径: %LOCALAPPDATA%\skill-sync\skills\

---

## 三、快速开始

### 3.1 启动程序

双击运行 skill-manager.exe 或在命令行中执行：

`
.\skill-manager.exe
`

### 3.2 首次登录

**界面显示：**
`
==================================================
Skill Manager - Enterprise Skill Distribution
==================================================

Current server: https://demo.egova.com.cn/skill-api

Options:
  1. Login
  2. Change Server Address
  3. Exit

Select option (1-3):
`

**操作步骤：**
1. 输入 1 选择登录
2. 输入 Redmine 用户名
3. 输入密码（不显示）
4. 登录成功后自动进入主菜单

**用户密码和redmine项目管理平台同步**

**登录命令行方式：**
`
.\skill-manager.exe login --username 你的用户名 --password 你的密码
`

### 3.3 自动登录

登录成功后，Token 会加密保存。下次启动程序时会自动识别：

`
==================================================
Skill Manager - Enterprise Skill Distribution
==================================================

Current server: https://demo.egova.com.cn/skill-api

Welcome back, majianquan!
You are already logged in.

Skill Manager - Main Menu
==================================================

Options:
  1. List all skills
  2. Install a skill
  ...
`

---

## 四、主要功能详解

### 4.1 查看可用 Skill 列表

**交互方式：**
- 登录后选择 1. List all skills

**命令行方式：**
`
.\skill-manager.exe list
`

**输出示例：**
`
Available skills (10):

--------------------------------------------------------------------------------
Name                           Description
--------------------------------------------------------------------------------
dingtalk-docs                  Read and sync DingTalk documents
dingtalk-knowledge-graph       Build knowledge graphs in MaxKB
elevator-data-validation       Residential elevator data validation
maxkb-knowledge-graph          MaxKB knowledge graph builder
skill-security-auditor         OpenClaw skill security auditor
tencent-docs                   腾讯文档操作能力
tongtu-assistant               通途平台数据操作助手
tongtu-pbc                     通途PBC导入助手
...
`

### 4.2 安装 Skill

**交互方式：**
1. 选择 2. Install a skill
2. 输入要安装的 Skill 名称

**命令行方式：**
`
.\skill-manager.exe install --skill tencent-docs
`

**强制重新安装：**
`
.\skill-manager.exe install --skill tencent-docs --force
`

**安装成功提示：**
`
Skill 'tencent-docs' installed successfully to C:\Users\用户名\.local\share\skill-sync\skills\tencent-docs
`

### 4.3 批量同步 Skill

一键同步所有可访问的 Skill：

**交互方式：**
- 选择 3. Sync all skills

**命令行方式：**
`
.\skill-manager.exe sync
`

### 4.4 修改 Skill 安装路径

**交互方式：**
1. 选择 4. Change skills directory（普通用户）或 6. Change skills directory（支持部门）
2. 系统会自动检测已安装的 Agent：
   - OpenCode
   - OpenClaw
   - WorkBuddy
3. 选择对应的 Agent 路径，或输入自定义路径

**命令行方式：**
`
.\skill-manager.exe config --skills-dir "D:\MySkills"
`

### 4.5 查看当前设置

**交互方式：**
- 选择 5. View current settings

**输出示例：**
`
Current settings:
  Server: https://demo.egova.com.cn/skill-api
  Skills directory: C:\Users\用户名\.local\share\skill-sync\skills
  Token file: C:\Users\用户名\.config\skill-sync\token.enc
  Timeout: 30
  Verify SSL: true
`

---

## 五、支持部门专属功能

如果您的账号属于支持部门（技术支持部），登录后会看到额外的维护菜单：

`
Options:
  1. List all skills
  2. Install a skill
  3. Sync all skills
  4. Upload a skill            [MAINTENANCE]
  5. View unsafe skills        [MAINTENANCE]
  6. Change skills directory
  7. View current settings
  8. Logout
  9. Exit
`

### 5.1 上传 Skill

**操作步骤：**
1. 选择 4. Upload a skill
2. 输入 Skill 目录路径
3. 输入版本号（可选，默认自动递增）
4. 等待上传和安全扫描完成

**上传流程：**
`
Uploading skill from: D:\my-skill

Upload successful!
  Version: 1.0.1
  Scan Status: passed
  Message: Skill published and available for download.
`

### 5.2 查看不安全 Skill

查看未通过安全扫描的 Skill：

**操作步骤：**
1. 选择 5. View unsafe skills

**输出示例：**
`
Unsafe Skills (2):
--------------------------------------------------------------------------------

  malicious-skill v1.0.0
    Uploaded: 2026-04-02 by admin
    Issues: 3 security violations detected
      - Prompt injection detected
      - Data exfiltration risk
      - Command execution found
`

---

## 六、常用命令速查表

| 命令 | 说明 |
|------|------|
| .\skill-manager.exe | 启动交互界面 |
| .\skill-manager.exe login --username 用户名 --password 密码 | 命令行登录 |
| .\skill-manager.exe logout | 登出并清除Token |
| .\skill-manager.exe status | 检查登录状态 |
| .\skill-manager.exe list | 列出可用Skill |
| .\skill-manager.exe install --skill 名称 | 安装指定Skill |
| .\skill-manager.exe install --skill 名称 --force | 强制重新安装 |
| .\skill-manager.exe sync | 同步所有Skill |
| .\skill-manager.exe config | 查看当前配置 |
| .\skill-manager.exe config --server URL | 修改服务器地址 |
| .\skill-manager.exe config --skills-dir 路径 | 修改安装路径 |

---

## 七、权限说明

### 7.1 普通用户
- 可见全局 Skill（约5-10个）
- 支持下载、安装、同步

### 7.2 支持部门用户
- 可见全局 Skill + 支持部门专属 Skill
- 支持上传 Skill
- 可查看未通过安全扫描的 Skill
- 当前支持部门用户：majianquan、lingcan、liaokun

---

## 八、常见问题

### Q1: 登录失败怎么办？
**原因：** 用户名或密码错误，或账号未在 Redmine 系统中创建

**解决：**
1. 确认 Redmine 账号正常
2. 检查用户名密码是否正确
3. 联系管理员确认账号状态

### Q2: 下载 Skill 失败？
**可能原因：**
- 网络连接问题
- 服务器暂时不可用
- Token 过期

**解决：**
1. 检查网络连接
2. 执行 logout 后重新登录
3. 联系服务器管理员

### Q3: 如何切换 Skill 安装路径？
**方法1：** 交互模式下选择 Change skills directory

**方法2：** 命令行：
`
.\skill-manager.exe config --skills-dir "新路径"
`

### Q4: 支持哪些 Agent？
当前支持以下 Agent 的 Skill 路径自动检测：
- OpenCode: %APPDATA%\opencode\skills
- OpenClaw: %USERPROFILE%\.openclaw\skills
- WorkBuddy: %APPDATA%\Tencent\WorkBuddy\skills

### Q5: Token 有效期多久？
Token 有效期为 **30天**。过期后需要重新登录。

---

## 九、安全说明

### 9.1 Token 加密存储
- Token 使用 Fernet 对称加密算法加密
- 加密密钥基于机器唯一标识生成
- 密码不会保存到内存或磁盘

### 9.2 用户信息存储
- 用户信息明文存储（仅包含用户名、角色）
- 不包含密码等敏感信息

### 9.3 Skill 安全扫描
所有上传的 Skill 都会经过安全扫描：
- 检测恶意代码
- 检测敏感信息泄露风险
- 检测命令注入等安全漏洞

未通过扫描的 Skill 会被标记为 unsafe，不会发布。

---

## 十、技术支持

### 10.1 服务器信息
- 服务器地址: https://demo.egova.com.cn/skill-api/
- 认证系统: Redmine (https://faq.egova.com.cn:7787/)

### 10.2 联系方式
如有问题，请联系技术支持部门。

---

## 附录：服务器 API 端点

| 端点 | 方法 | 说明 |
|------|------|------|
| /auth/login | POST | 用户登录 |
| /auth/validate | GET | 验证Token |
| /auth/refresh | POST | 刷新Token |
| /skills | GET | 获取Skill列表 |
| /skills/:name | GET | 获取Skill详情 |
| /skills/:name/:version/download | GET | 下载Skill包 |
| /skills/:name/upload | POST | 上传Skill（支持部门） |

---

**文档版本:** 1.0
**最后更新:** 2026-04-02
**作者:** Skill Distribution System
