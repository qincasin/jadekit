# Changelog

## [1.0.3] - 2026-06-10

### Fixed
- 修复 GitHub API 限流或请求失败时误显示为已是最新版本的问题
- 更新检查失败时保留并展示具体错误信息，便于判断是否被限流
- 修复版本升级脚本未同步更新官网更新日志的问题

## [1.0.2] - 2026-06-10

### Fixed
- 修复 Windows 构建失败问题

## [1.0.0] - 2026-06-10

### Added
- JadeKit 作为独立项目正式发布，完成品牌、图标与仓库切换
- 保留 Provider、Workspace、Skills、Prompts 等核心配置能力
- 新增 Antigravity 账号管理、状态查看、切换与预热能力
- 提供独立迁移脚本，仅迁移服务商与应用设置，不迁移聊天记录

### Improved
- 官网、GitHub Pages 与 Release 流程切换到新的 jadekit 仓库
- 更新日志页面改为直接读取 CHANGELOG.md 作为单一来源

### Fixed
- 清理旧仓库遗留的版本展示，避免官网版本列表与实际发布历史不一致
