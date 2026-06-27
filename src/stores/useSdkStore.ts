import {create} from 'zustand';
import {invoke} from '@tauri-apps/api/core';
import {listen, type UnlistenFn} from '@tauri-apps/api/event';
import type {
    NodeRuntimeInstallDoneEvent,
    NodeRuntimeInstallLogEvent,
    NodeRuntimeStatus,
    SdkInstallDoneEvent,
    SdkInstallLogEvent,
    SdkStatus,
} from '../types/chat';

interface SdkState {
    statuses: SdkStatus[];
    loading: boolean;
    /** 正在安装的 sdkId（null 表示无安装进行中） */
    installing: string | null;
    /** 安装日志（按行累积，最多保留最近 200 行） */
    logs: string[];
    nodeRuntimeStatus: NodeRuntimeStatus | null;
    nodeRuntimeInstalling: boolean;
    nodeRuntimeLogs: string[];
    error: string | null;
    initialized: boolean;

    init: () => Promise<void>;
    refresh: () => Promise<void>;
    install: (sdkId: string, version?: string) => Promise<void>;
    installNodeRuntime: () => Promise<void>;
    uninstall: (sdkId: string) => Promise<void>;
    clearLogs: () => void;
}

let unlisteners: UnlistenFn[] = [];
const MAX_LOG_LINES = 200;

export const useSdkStore = create<SdkState>((set, get) => ({
    statuses: [],
    loading: false,
    installing: null,
    logs: [],
    nodeRuntimeStatus: null,
    nodeRuntimeInstalling: false,
    nodeRuntimeLogs: [],
    error: null,
    initialized: false,

    init: async () => {
        if (get().initialized) return;
        set({ initialized: true });

        unlisteners.forEach((u) => u());
        unlisteners = [];

        const logUn = await listen<SdkInstallLogEvent>('chat://sdk-install-log', (event) => {
            set((state) => {
                const next = [...state.logs, event.payload.line];
                if (next.length > MAX_LOG_LINES) next.splice(0, next.length - MAX_LOG_LINES);
                return { logs: next };
            });
        });

        const doneUn = await listen<SdkInstallDoneEvent>(
            'chat://sdk-install-done',
            (event) => {
                const { success, error } = event.payload;
                set({
                    installing: null,
                    error: success ? null : error || '安装失败',
                });
                // 安装结束后刷新状态
                get().refresh();
            },
        );

        const runtimeLogUn = await listen<NodeRuntimeInstallLogEvent>(
            'chat://node-runtime-install-log',
            (event) => {
                set((state) => {
                    const next = [...state.nodeRuntimeLogs, event.payload.line];
                    if (next.length > MAX_LOG_LINES) next.splice(0, next.length - MAX_LOG_LINES);
                    return { nodeRuntimeLogs: next };
                });
            },
        );

        const runtimeDoneUn = await listen<NodeRuntimeInstallDoneEvent>(
            'chat://node-runtime-install-done',
            (event) => {
                const { success, error, status } = event.payload;
                set({
                    nodeRuntimeInstalling: false,
                    nodeRuntimeStatus: status ?? get().nodeRuntimeStatus,
                    error: success ? null : error || 'Node.js 运行环境安装失败',
                });
                get().refresh();
            },
        );

        unlisteners = [logUn, doneUn, runtimeLogUn, runtimeDoneUn];
        await get().refresh();
    },

    refresh: async () => {
        set({ loading: true });
        try {
            const [statuses, nodeRuntimeStatus] = await Promise.all([
                invoke<SdkStatus[]>('chat_sdk_status'),
                invoke<NodeRuntimeStatus>('chat_node_runtime_status'),
            ]);
            set({ statuses, nodeRuntimeStatus, loading: false });
        } catch (e) {
            set({ error: String(e), loading: false });
        }
    },

    install: async (sdkId, version) => {
        if (get().installing) return;
        set({ installing: sdkId, logs: [], error: null });
        try {
            // 注意：该命令在安装完成后才 resolve（含 daemon 重启），
            // 进度通过事件实时推送。
            await invoke('chat_install_sdk', { sdkId, version });
        } catch (e) {
            set({ installing: null, error: String(e) });
        }
    },

    installNodeRuntime: async () => {
        if (get().nodeRuntimeInstalling) return;
        set({ nodeRuntimeInstalling: true, nodeRuntimeLogs: [], error: null });
        try {
            await invoke('chat_install_node_runtime');
        } catch (e) {
            set({ nodeRuntimeInstalling: false, error: String(e) });
        }
    },

    uninstall: async (sdkId) => {
        try {
            await invoke('chat_uninstall_sdk', { sdkId });
            await get().refresh();
        } catch (e) {
            set({ error: String(e) });
        }
    },

    clearLogs: () => set({ logs: [], nodeRuntimeLogs: [] }),
}));
