import { Octokit } from '@octokit/rest';
import { execSync } from 'child_process';
import type { LinkedPR, RepoInfo, PRCreateOptions, PRMergeOptions } from '../types.js';

let octokitInstance: Octokit | null = null;

/**
 * Get GitHub token from environment or gh CLI
 */
export function getGitHubToken(): string {
  // Try environment variable first
  if (process.env.GITHUB_TOKEN) {
    return process.env.GITHUB_TOKEN;
  }

  // Try gh CLI
  try {
    const token = execSync('gh auth token', { encoding: 'utf-8' }).trim();
    if (token) {
      return token;
    }
  } catch {
    // gh CLI not available or not authenticated
  }

  throw new Error(
    'GitHub token not found. Set GITHUB_TOKEN environment variable or run "gh auth login"'
  );
}

/**
 * Get or create Octokit instance
 */
export function getOctokit(): Octokit {
  if (!octokitInstance) {
    const token = getGitHubToken();
    octokitInstance = new Octokit({ auth: token });
  }
  return octokitInstance;
}

/**
 * Create a pull request
 */
export async function createPullRequest(
  owner: string,
  repo: string,
  head: string,
  base: string,
  title: string,
  body: string,
  draft = false
): Promise<{ number: number; url: string }> {
  const octokit = getOctokit();
  const response = await octokit.pulls.create({
    owner,
    repo,
    head,
    base,
    title,
    body,
    draft,
  });

  return {
    number: response.data.number,
    url: response.data.html_url,
  };
}

/**
 * Update a pull request body
 */
export async function updatePullRequestBody(
  owner: string,
  repo: string,
  pullNumber: number,
  body: string
): Promise<void> {
  const octokit = getOctokit();
  await octokit.pulls.update({
    owner,
    repo,
    pull_number: pullNumber,
    body,
  });
}

/**
 * Get pull request details
 */
export async function getPullRequest(
  owner: string,
  repo: string,
  pullNumber: number
): Promise<{
  number: number;
  url: string;
  title: string;
  body: string;
  state: 'open' | 'closed';
  merged: boolean;
  mergeable: boolean | null;
  head: { ref: string; sha: string };
  base: { ref: string };
}> {
  const octokit = getOctokit();
  const response = await octokit.pulls.get({
    owner,
    repo,
    pull_number: pullNumber,
  });

  return {
    number: response.data.number,
    url: response.data.html_url,
    title: response.data.title,
    body: response.data.body ?? '',
    state: response.data.state as 'open' | 'closed',
    merged: response.data.merged,
    mergeable: response.data.mergeable,
    head: {
      ref: response.data.head.ref,
      sha: response.data.head.sha,
    },
    base: {
      ref: response.data.base.ref,
    },
  };
}

/**
 * Get reviews for a pull request
 */
export async function getPullRequestReviews(
  owner: string,
  repo: string,
  pullNumber: number
): Promise<{ state: string; user: string }[]> {
  const octokit = getOctokit();
  const response = await octokit.pulls.listReviews({
    owner,
    repo,
    pull_number: pullNumber,
  });

  return response.data.map((review) => ({
    state: review.state,
    user: review.user?.login ?? 'unknown',
  }));
}

/**
 * Check if a PR is approved
 */
export async function isPullRequestApproved(
  owner: string,
  repo: string,
  pullNumber: number
): Promise<boolean> {
  const reviews = await getPullRequestReviews(owner, repo, pullNumber);
  // Consider approved if at least one APPROVED review and no CHANGES_REQUESTED
  const hasApproval = reviews.some((r) => r.state === 'APPROVED');
  const hasChangesRequested = reviews.some((r) => r.state === 'CHANGES_REQUESTED');
  return hasApproval && !hasChangesRequested;
}

/**
 * Get combined status checks for a PR
 */
export async function getStatusChecks(
  owner: string,
  repo: string,
  ref: string
): Promise<{ state: 'success' | 'failure' | 'pending'; statuses: { context: string; state: string }[] }> {
  const octokit = getOctokit();
  const response = await octokit.repos.getCombinedStatusForRef({
    owner,
    repo,
    ref,
  });

  return {
    state: response.data.state as 'success' | 'failure' | 'pending',
    statuses: response.data.statuses.map((s) => ({
      context: s.context,
      state: s.state,
    })),
  };
}

/**
 * Merge a pull request
 */
export async function mergePullRequest(
  owner: string,
  repo: string,
  pullNumber: number,
  options: PRMergeOptions = {}
): Promise<boolean> {
  const octokit = getOctokit();
  const mergeMethod = options.method ?? 'merge';

  try {
    await octokit.pulls.merge({
      owner,
      repo,
      pull_number: pullNumber,
      merge_method: mergeMethod,
    });

    // Delete branch if requested
    if (options.deleteBranch) {
      const pr = await getPullRequest(owner, repo, pullNumber);
      try {
        await octokit.git.deleteRef({
          owner,
          repo,
          ref: `heads/${pr.head.ref}`,
        });
      } catch {
        // Branch deletion failure is not critical
      }
    }

    return true;
  } catch (error) {
    return false;
  }
}

/**
 * Get full linked PR information
 */
export async function getLinkedPRInfo(
  owner: string,
  repo: string,
  pullNumber: number,
  repoName: string
): Promise<LinkedPR> {
  const pr = await getPullRequest(owner, repo, pullNumber);
  const approved = await isPullRequestApproved(owner, repo, pullNumber);
  const checks = await getStatusChecks(owner, repo, pr.head.sha);

  let state: 'open' | 'closed' | 'merged';
  if (pr.merged) {
    state = 'merged';
  } else {
    state = pr.state;
  }

  return {
    repoName,
    owner,
    repo,
    number: pr.number,
    url: pr.url,
    state,
    approved,
    checksPass: checks.state === 'success',
    mergeable: pr.mergeable ?? false,
  };
}

/**
 * Find PRs with a specific branch name
 */
export async function findPRByBranch(
  owner: string,
  repo: string,
  branch: string
): Promise<{ number: number; url: string } | null> {
  const octokit = getOctokit();
  const response = await octokit.pulls.list({
    owner,
    repo,
    head: `${owner}:${branch}`,
    state: 'open',
  });

  if (response.data.length > 0) {
    return {
      number: response.data[0].number,
      url: response.data[0].html_url,
    };
  }
  return null;
}

/**
 * Create PRs for all repos with changes
 */
export async function createLinkedPRs(
  repos: RepoInfo[],
  branchName: string,
  options: PRCreateOptions,
  manifestPRNumber?: number
): Promise<LinkedPR[]> {
  const linkedPRs: LinkedPR[] = [];

  for (const repo of repos) {
    // Check if PR already exists
    const existing = await findPRByBranch(repo.owner, repo.repo, branchName);
    if (existing) {
      const info = await getLinkedPRInfo(repo.owner, repo.repo, existing.number, repo.name);
      linkedPRs.push(info);
      continue;
    }

    // Create title with manifest reference if available
    const title = manifestPRNumber
      ? `[manifest#${manifestPRNumber}] ${options.title}`
      : options.title;

    // Create body with cross-reference
    let body = options.body ?? '';
    if (manifestPRNumber) {
      body = `Part of manifest PR #${manifestPRNumber}\n\n${body}`;
    }

    const pr = await createPullRequest(
      repo.owner,
      repo.repo,
      branchName,
      options.base ?? repo.default_branch,
      title,
      body,
      options.draft
    );

    const info = await getLinkedPRInfo(repo.owner, repo.repo, pr.number, repo.name);
    linkedPRs.push(info);
  }

  return linkedPRs;
}

/**
 * Generate manifest PR body with linked PR table
 */
export function generateManifestPRBody(
  title: string,
  linkedPRs: LinkedPR[],
  additionalBody?: string
): string {
  const prTable = linkedPRs
    .map((pr) => {
      const statusIcon = pr.state === 'merged' ? ':white_check_mark:' : pr.state === 'open' ? ':hourglass:' : ':x:';
      const approvalIcon = pr.approved ? ':white_check_mark:' : ':hourglass:';
      const checksIcon = pr.checksPass ? ':white_check_mark:' : ':hourglass:';
      return `| ${pr.repoName} | [#${pr.number}](${pr.url}) | ${statusIcon} ${pr.state} | ${approvalIcon} | ${checksIcon} |`;
    })
    .join('\n');

  const prLinks = linkedPRs.map((pr) => `${pr.repoName}#${pr.number}`).join(',');

  return `## Cross-Repository PR

${additionalBody ?? ''}

### Linked Pull Requests

| Repository | PR | Status | Approved | Checks |
|------------|-----|--------|----------|--------|
${prTable}

**Merge Policy:** All-or-nothing - all linked PRs must be approved before merge.

---
<!-- codi-repo:links:${prLinks} -->
`;
}

/**
 * Parse linked PRs from manifest PR body
 */
export function parseLinkedPRsFromBody(body: string): { repoName: string; number: number }[] {
  const match = body.match(/<!-- codi-repo:links:(.+?) -->/);
  if (!match) {
    return [];
  }

  const links = match[1].split(',');
  return links.map((link) => {
    const [repoName, numStr] = link.split('#');
    return { repoName, number: parseInt(numStr, 10) };
  });
}
