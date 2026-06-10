# Versioning Guide

JadeKit uses one unified version script:

```bash
npm run bump <version> <major|minor|patch> "description"
npm run bump <major|minor|patch> "description"
```

The script updates every project version source and asks for confirmation before writing.

## Version States

Use `-SNAPSHOT` for development builds and remove it only when preparing a release.

| Situation | Version example | Command example |
| --- | --- | --- |
| Initial development baseline | `1.0.0-SNAPSHOT` | already set |
| Release current baseline | `1.0.0` | `npm run bump 1.0.0 patch "Release 1.0.0"` |
| Start next feature version | `1.1.0-SNAPSHOT` | `npm run bump 1.1.0-SNAPSHOT minor "Start next feature version"` |
| Release next feature version | `1.1.0` | `npm run bump 1.1.0 minor "Release 1.1.0"` |
| Start next bugfix version | `1.0.1-SNAPSHOT` | `npm run bump 1.0.1-SNAPSHOT patch "Start next bugfix version"` |
| Release next bugfix version | `1.0.1` | `npm run bump 1.0.1 patch "Release 1.0.1"` |

## Choosing X/Y/Z

- `major` updates `x`: incompatible changes, data model breaks, or migration-heavy releases.
- `minor` updates `y`: new features that remain compatible.
- `patch` updates `z`: fixes, small polish, and compatible maintenance work.

When a feature or fix is complete, do not bump automatically. Ask the user:

> 本功能已经完成，是否需要升级版本？如果需要，升级 `major`、`minor` 还是 `patch`？目标是开发快照还是正式发布？

## Running It Yourself

You can run the script directly:

```bash
npm run bump 1.1.0-SNAPSHOT minor "Start next feature version"
```

Or ask the agent to run it. The agent should confirm:

- target version,
- `major` / `minor` / `patch`,
- snapshot or release,
- short changelog description.

## Files Updated By The Script

- `package.json`
- `package-lock.json`
- `website/package.json`
- `website/package-lock.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/tauri.conf.json`
- `CHANGELOG.md`

Do not hand-edit these version values across files unless the user explicitly asks for a one-off correction.

## Release Order

The version change must be committed before creating the git tag because GitHub Actions reads the package version from `src-tauri/tauri.conf.json`.

```bash
npm run bump 1.0.0 patch "Release 1.0.0"
git diff
git add .
git commit -m "chore: release 1.0.0"
git tag -a v1.0.0
git push origin main
git push origin v1.0.0
```
