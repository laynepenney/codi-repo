import type { LinkedPR, ManifestPR, RepoInfo, StateFile } from '../types.js';
import {
  getLinkedPRInfo,
  getPullRequest,
  updatePullRequestBody,
  generateManifestPRBody,
  parseLinkedPRsFromBody,
  mergePullRequest,
  isPullRequestApproved,
} from './github.js';
import { loadState, saveState, getAllRepoInfo } from './manifest.js';
import type { Manifest } from '../types.js';

/**
 * Link a branch to a manifest PR in state
 */
export async function linkBranchToManifestPR(
  rootDir: string,
  branchName: string,
  manifestPRNumber: number
): Promise<void> {
  const state = await loadState(rootDir);
  state.branchToPR[branchName] = manifestPRNumber;
  state.currentManifestPR = manifestPRNumber;
  await saveState(rootDir, state);
}

/**
 * Save linked PRs for a manifest PR
 */
export async function saveLinkedPRs(
  rootDir: string,
  manifestPRNumber: number,
  linkedPRs: LinkedPR[]
): Promise<void> {
  const state = await loadState(rootDir);
  state.prLinks[manifestPRNumber] = linkedPRs;
  await saveState(rootDir, state);
}

/**
 * Get the manifest PR number for a branch
 */
export async function getManifestPRForBranch(
  rootDir: string,
  branchName: string
): Promise<number | undefined> {
  const state = await loadState(rootDir);
  return state.branchToPR[branchName];
}

/**
 * Get linked PRs for a manifest PR from state
 */
export async function getLinkedPRsFromState(
  rootDir: string,
  manifestPRNumber: number
): Promise<LinkedPR[] | undefined> {
  const state = await loadState(rootDir);
  return state.prLinks[manifestPRNumber];
}

/**
 * Refresh linked PR status from GitHub
 */
export async function refreshLinkedPRStatus(
  manifest: Manifest,
  rootDir: string,
  linkedPRs: LinkedPR[]
): Promise<LinkedPR[]> {
  const repos = getAllRepoInfo(manifest, rootDir);
  const repoMap = new Map(repos.map((r) => [r.name, r]));

  const refreshed = await Promise.all(
    linkedPRs.map(async (pr) => {
      const repoInfo = repoMap.get(pr.repoName);
      if (!repoInfo) {
        return pr; // Keep old info if repo not found
      }
      return getLinkedPRInfo(repoInfo.owner, repoInfo.repo, pr.number, pr.repoName);
    })
  );

  return refreshed;
}

/**
 * Get full manifest PR info with refreshed linked PR status
 */
export async function getManifestPRInfo(
  manifest: Manifest,
  rootDir: string,
  manifestOwner: string,
  manifestRepo: string,
  manifestPRNumber: number
): Promise<ManifestPR> {
  const pr = await getPullRequest(manifestOwner, manifestRepo, manifestPRNumber);

  // Parse linked PRs from body
  const parsedLinks = parseLinkedPRsFromBody(pr.body);
  const repos = getAllRepoInfo(manifest, rootDir);
  const repoMap = new Map(repos.map((r) => [r.name, r]));

  // Get fresh status for each linked PR
  const linkedPRs = await Promise.all(
    parsedLinks.map(async ({ repoName, number }) => {
      const repoInfo = repoMap.get(repoName);
      if (!repoInfo) {
        // Return placeholder if repo not in manifest
        return {
          repoName,
          owner: '',
          repo: '',
          number,
          url: '',
          state: 'closed' as const,
          approved: false,
          checksPass: false,
          mergeable: false,
        };
      }
      return getLinkedPRInfo(repoInfo.owner, repoInfo.repo, number, repoName);
    })
  );

  // Determine overall state
  let state: 'open' | 'closed' | 'merged';
  if (pr.merged) {
    state = 'merged';
  } else {
    state = pr.state;
  }

  // Check if ready to merge (all linked PRs approved and checks pass)
  const readyToMerge =
    state === 'open' &&
    linkedPRs.every((p) => p.approved && p.checksPass && p.mergeable && p.state === 'open');

  return {
    number: pr.number,
    url: pr.url,
    title: pr.title,
    linkedPRs,
    state,
    readyToMerge,
  };
}

/**
 * Update manifest PR body with current linked PR status
 */
export async function syncManifestPRBody(
  manifest: Manifest,
  rootDir: string,
  manifestOwner: string,
  manifestRepo: string,
  manifestPRNumber: number
): Promise<void> {
  const manifestPR = await getManifestPRInfo(
    manifest,
    rootDir,
    manifestOwner,
    manifestRepo,
    manifestPRNumber
  );

  // Get original PR for title
  const pr = await getPullRequest(manifestOwner, manifestRepo, manifestPRNumber);

  // Generate updated body
  const newBody = generateManifestPRBody(pr.title, manifestPR.linkedPRs);

  // Update PR
  await updatePullRequestBody(manifestOwner, manifestRepo, manifestPRNumber, newBody);
}

/**
 * Merge all linked PRs in order, then merge manifest PR
 */
export async function mergeAllLinkedPRs(
  manifest: Manifest,
  rootDir: string,
  manifestOwner: string,
  manifestRepo: string,
  manifestPRNumber: number,
  options: { method?: 'merge' | 'squash' | 'rebase'; deleteBranch?: boolean } = {}
): Promise<{
  success: boolean;
  mergedPRs: { repoName: string; number: number }[];
  failedPR?: { repoName: string; number: number; error: string };
}> {
  const manifestPR = await getManifestPRInfo(
    manifest,
    rootDir,
    manifestOwner,
    manifestRepo,
    manifestPRNumber
  );

  if (!manifestPR.readyToMerge) {
    const notReady = manifestPR.linkedPRs.find(
      (p) => !p.approved || !p.checksPass || !p.mergeable || p.state !== 'open'
    );
    return {
      success: false,
      mergedPRs: [],
      failedPR: notReady
        ? {
            repoName: notReady.repoName,
            number: notReady.number,
            error: !notReady.approved
              ? 'Not approved'
              : !notReady.checksPass
                ? 'Checks not passing'
                : !notReady.mergeable
                  ? 'Not mergeable'
                  : 'PR not open',
          }
        : undefined,
    };
  }

  const mergedPRs: { repoName: string; number: number }[] = [];

  // Merge each linked PR
  for (const linkedPR of manifestPR.linkedPRs) {
    const merged = await mergePullRequest(linkedPR.owner, linkedPR.repo, linkedPR.number, options);
    if (!merged) {
      return {
        success: false,
        mergedPRs,
        failedPR: {
          repoName: linkedPR.repoName,
          number: linkedPR.number,
          error: 'Merge failed',
        },
      };
    }
    mergedPRs.push({ repoName: linkedPR.repoName, number: linkedPR.number });
  }

  // Merge manifest PR
  const manifestMerged = await mergePullRequest(
    manifestOwner,
    manifestRepo,
    manifestPRNumber,
    options
  );
  if (!manifestMerged) {
    return {
      success: false,
      mergedPRs,
      failedPR: {
        repoName: 'manifest',
        number: manifestPRNumber,
        error: 'Manifest PR merge failed',
      },
    };
  }

  mergedPRs.push({ repoName: 'manifest', number: manifestPRNumber });

  return {
    success: true,
    mergedPRs,
  };
}

/**
 * Check if all linked PRs are in sync (same branch name exists)
 */
export async function checkBranchSync(
  repos: RepoInfo[],
  branchName: string
): Promise<{ inSync: boolean; missing: string[] }> {
  const { branchExists } = await import('./git.js');

  const results = await Promise.all(
    repos.map(async (repo) => {
      const exists = await branchExists(repo.absolutePath, branchName);
      return { name: repo.name, exists };
    })
  );

  const missing = results.filter((r) => !r.exists).map((r) => r.name);

  return {
    inSync: missing.length === 0,
    missing,
  };
}
