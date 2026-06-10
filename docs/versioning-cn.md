# 版本升级指南

JadeKit 使用一个统一版本脚本：

```bash
npm run bump <版本号> <major|minor|patch> "描述"
npm run bump <major|minor|patch> "描述"
```

脚本会统一更新项目里的所有版本来源，并在真正写入前要求确认。

## 版本状态

开发构建使用 `-SNAPSHOT`，准备正式发布时再去掉。

| 场景 | 版本示例 | 命令示例 |
| --- | --- | --- |
| 初始开发基线 | `1.0.0-SNAPSHOT` | 已设置 |
| 发布当前基线 | `1.0.0` | `npm run bump 1.0.0 patch "发布 1.0.0"` |
| 开始下一个功能版本 | `1.1.0-SNAPSHOT` | `npm run bump 1.1.0-SNAPSHOT minor "开始下一个功能版本"` |
| 发布下一个功能版本 | `1.1.0` | `npm run bump 1.1.0 minor "发布 1.1.0"` |
| 开始下一个修复版本 | `1.0.1-SNAPSHOT` | `npm run bump 1.0.1-SNAPSHOT patch "开始下一个修复版本"` |
| 发布下一个修复版本 | `1.0.1` | `npm run bump 1.0.1 patch "发布 1.0.1"` |

## 如何选择 X/Y/Z

- `major` 升级 `x`：不兼容变更、数据模型破坏、迁移成本较大的发布。
- `minor` 升级 `y`：兼容的新功能。
- `patch` 升级 `z`：问题修复、小优化、兼容维护。

功能或修复完成后，不要自动升级版本。应先询问用户：

> 本功能已经完成，是否需要升级版本？如果需要，升级 `major`、`minor` 还是 `patch`？目标是开发快照还是正式发布？

## 用户自己运行

用户可以直接运行脚本：

```bash
npm run bump 1.1.0-SNAPSHOT minor "开始下一个功能版本"
```

也可以让 agent 代跑。agent 需要先确认：

- 目标版本号；
- `major` / `minor` / `patch`；
- 开发快照还是正式发布；
- 简短 changelog 描述。

## 脚本会更新的文件

- `package.json`
- `package-lock.json`
- `website/package.json`
- `website/package-lock.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/tauri.conf.json`
- `website/src/pages/Changelog.tsx`

除非用户明确要求做一次性修正，否则不要手动分散修改这些版本号。

## 发布顺序

版本变更必须先提交，再创建 git tag。GitHub Actions 构建安装包时读取的是 `src-tauri/tauri.conf.json` 里的版本号。

```bash
npm run bump 1.0.0 patch "发布 1.0.0"
git diff
git add .
git commit -m "chore: release 1.0.0"
git tag -a v1.0.0
git push origin main
git push origin v1.0.0
```
