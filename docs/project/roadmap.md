# JadeKit 项目路线图

## 当前状态

JadeKit 已进入稳定迭代阶段，核心功能模块已全部实现并投入使用。

## 已完成功能

### 多服务商管理
- Provider 统一 CRUD（SQLite 数据库存储）
- 一键切换，自动写入 Claude / Codex / Gemini 配置文件
- 健康检查、配置预览、导入导出
- Deep Link 一键导入（`jadekit://` / `ccswitch://`）

### 内置本地代理
- Rust + Axum 高性能 HTTP 反向代理
- 健康检查、熔断器、故障切换（Failover）
- Thinking Budget 修正、Model 映射
- 请求/响应日志与可视化监控

### Antigravity 账号管理
- Google OAuth2 浏览器登录
- 多账号管理、Token 自动刷新
- 配额实时监控（FREE/PRO/ULTRA 层级）
- 账号预热、批量操作、操作日志

### MCP 服务器管理
- 数据库版 MCP 配置管理（v2）
- 多工具同步（Claude JSON / Codex TOML / Gemini JSON）
- MCP 配置导入与校验

### Prompt / 技能 / 子代理
- 数据库版 Prompt 管理（v2），支持启用/禁用、跨应用同步
- 技能发现、安装、卸载、沙盒运行
- 子代理模板 CRUD

### 基础设施
- SQLite 数据存储（v3，从 JSON 迁移完成）
- 自动更新检查 + 静默下载 + 安装
- WebDAV 远程备份 + 本地自动备份
- 系统托盘常驻
- 中英文国际化
- 暗黑模式（View Transition 动画）
- 多终端支持（Ghostty / iTerm2 / Warp / Terminal 等）
- 仪表盘统计与用量追踪
- 数据库备份/恢复

## 后续规划

### 短期
- [ ] 代理规则高级配置（请求头改写、路径路由）
- [ ] Provider 分组管理
- [ ] 用量统计报表导出

### 中期
- [ ] 插件系统（第三方扩展）
- [ ] 团队协作（共享 Provider 配置）
- [ ] 云同步配置

### 长期
- [ ] 更多 AI 工具支持（Cursor、Windsurf 等）
- [ ] 配置模板市场
- [ ] 性能监控仪表盘
