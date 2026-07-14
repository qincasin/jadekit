# Changelog

## [1.2.0] - 2026-07-14

### Added
- Release 1.2.0 - Helm Cockpit Phase 4 Complete

## [1.1.0] - 2026-06-27

### Added
- 引入 Agent 问答聊天功能,内置聊天面板与会话状态/大纲展示
- 供应商 per-model 1M 上下文支持:后端 `[1M]` 后缀解析、one_m_context 持久化、表单按模型角色声明
- 内置官方订阅 Provider:列表激活、切换时清除供应商 env 回落 OAuth
- 供应商配置预览可编辑,并与表单双向同步
- Antigravity IDE 集成增强:state.vscdb 凭据注入、设备指纹、当前账号真相源
- 品牌焕新,产品定位统一为 AI Agent Routing Kit

### Changed
- 全量替换应用与站点图标资源
- 聊天状态面板(StatusPanel)拖拽重构:改用自定义鼠标事件,新增落点高亮与插入位反馈,修复动画下定位漂移

### Fixed
- 官方订阅跳过探活检测;排序下标排除官方项,禁止官方项作为拖拽目标

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
