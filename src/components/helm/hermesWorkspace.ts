import {
  loadChatWorkspaceStatus,
  type ChatWorkspaceStatus,
} from '../../utils/chatWorkspaceStatus';

export type WorkspaceStatusLoader = (cwd?: string | null) => Promise<ChatWorkspaceStatus>;

export class HermesWorkspaceLoadError extends Error {
  constructor() {
    super('Failed to load workspace status');
    this.name = 'HermesWorkspaceLoadError';
  }
}

export async function resolveHermesWorkspaceRoot(
  selectedFolder: string | null | undefined,
  loadWorkspaceStatus: WorkspaceStatusLoader = loadChatWorkspaceStatus,
): Promise<string | null> {
  const normalizedFolder = selectedFolder?.trim();
  if (!normalizedFolder) return null;

  try {
    const status = await loadWorkspaceStatus(normalizedFolder);
    const gitRoot = status.isGitRepository ? status.gitRoot?.trim() : null;
    return gitRoot || null;
  } catch {
    throw new HermesWorkspaceLoadError();
  }
}
