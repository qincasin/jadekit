import { invoke } from '@tauri-apps/api/core';
import {
  TaskDto,
  DispatchDto,
  RunShowDto,
  SweepReportDto,
  HermesRunOpts,
  TaskListFilterDto,
} from '../types/hermes';

/**
 * 启动一次编排 run，立即返回 run_id；进展通过 `hermes://run` / `hermes://task` 事件流推送
 */
export async function run(goal: string, opts?: HermesRunOpts): Promise<string> {
  return await invoke<string>('hermes_run', { goal, opts });
}

/**
 * 列出任务（可按 status / ready 过滤）
 */
export async function taskList(filter?: TaskListFilterDto): Promise<TaskDto[]> {
  return await invoke<TaskDto[]>('hermes_task_list', { filter });
}

/**
 * 取一条派发上下文（按 dispatch_id 查）
 */
export async function dispatchShow(dispatchId: string): Promise<DispatchDto> {
  return await invoke<DispatchDto>('hermes_dispatch_show', { dispatchId });
}

/**
 * 解决一个决策门（resolution 写入 Store，status → Resolved）
 */
export async function gateResolve(gateId: string, resolution: string): Promise<void> {
  await invoke<void>('hermes_gate_resolve', { gateId, resolution });
}

/**
 * 取消指定 run（置 cancel 标志）。run() 启动前置位 → pre-loop 命中标 Failed；循环中置位 → tick-top 命中标 Cancelled。
 */
export async function runStop(runId: string): Promise<void> {
  await invoke<void>('hermes_run_stop', { runId });
}

/**
 * 取消指定 run（mid-run 语义）：置 cancel 标志，Coordinator 下一轮 tick 检查到即 abort 在飞 dispatch + 标 Cancelled
 */
export async function runCancel(runId: string): Promise<void> {
  await invoke<void>('hermes_run_cancel', { runId });
}

/**
 * 取一条 run 概览 + 任务计数（驾驶舱顶部用）
 */
export async function runShow(runId: string): Promise<RunShowDto> {
  return await invoke<RunShowDto>('hermes_run_show', { runId });
}

/**
 * 列出当前活跃派发上下文（status = Dispatched），驾驶舱 Roster 用
 */
export async function agentList(): Promise<DispatchDto[]> {
  return await invoke<DispatchDto[]>('hermes_agent_list');
}

/**
 * 手动触发一次 run 的 worktree 清扫
 */
export async function runCleanup(runId: string): Promise<SweepReportDto> {
  return await invoke<SweepReportDto>('hermes_run_cleanup', { runId });
}
