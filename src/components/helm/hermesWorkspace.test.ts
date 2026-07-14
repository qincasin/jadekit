import { describe, expect, it, vi } from 'vitest';
import { HermesWorkspaceLoadError, resolveHermesWorkspaceRoot } from './hermesWorkspace';

describe('resolveHermesWorkspaceRoot', () => {
  it('returns the normalized Git root for a selected repository folder', async () => {
    const loadWorkspaceStatus = async () => ({
      isGitRepository: true,
      gitRoot: '  /workspace/jadekit  ',
      gitBranch: 'main',
    });

    expect(await resolveHermesWorkspaceRoot('/workspace/jadekit/packages/app', loadWorkspaceStatus))
      .toBe('/workspace/jadekit');
  });

  it('returns null when the selected folder is not in a Git repository', async () => {
    const loadWorkspaceStatus = vi.fn().mockResolvedValue({
      isGitRepository: false,
      gitRoot: '/workspace/not-a-repository',
      gitBranch: null,
    });

    await expect(resolveHermesWorkspaceRoot('/workspace/not-a-repository', loadWorkspaceStatus)).resolves.toBeNull();
    expect(loadWorkspaceStatus).toHaveBeenCalledWith('/workspace/not-a-repository');
  });

  it('does not load workspace status when no folder is selected', async () => {
    const loadWorkspaceStatus = vi.fn();

    await expect(resolveHermesWorkspaceRoot('  ', loadWorkspaceStatus)).resolves.toBeNull();
    expect(loadWorkspaceStatus).not.toHaveBeenCalled();
  });

  it('throws a controlled error when loading repository status fails', async () => {
    const loadWorkspaceStatus = vi.fn().mockRejectedValue(new Error('Tauri command failed'));

    await expect(resolveHermesWorkspaceRoot('/workspace/jadekit', loadWorkspaceStatus))
      .rejects.toBeInstanceOf(HermesWorkspaceLoadError);
  });
});
