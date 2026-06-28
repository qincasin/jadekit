// 扇出回滚纯函数：从本轮已成功创建的 worktree 列表中提取需要回滚的 path。
// 结构化参数 `{path: string}`，与具体 worktree 类型解耦，便于单元测试。
export function worktreesToRollback(created: {path: string}[]): string[] {
    return created.map((item) => item.path);
}
