import chalk from 'chalk';
import ora from 'ora';
import { loadManifest, getAllRepoInfo } from '../lib/manifest.js';
import { checkoutBranchInAllRepos, pathExists, branchExists } from '../lib/git.js';
import type { RepoInfo } from '../types.js';

interface CheckoutOptions {
  create?: boolean;
}

/**
 * Checkout a branch across all repositories
 */
export async function checkout(branchName: string, options: CheckoutOptions = {}): Promise<void> {
  const { manifest, rootDir } = await loadManifest();
  const repos = getAllRepoInfo(manifest, rootDir);

  // Filter to cloned repos only
  const clonedRepos: RepoInfo[] = [];
  for (const repo of repos) {
    if (await pathExists(repo.absolutePath)) {
      clonedRepos.push(repo);
    }
  }

  if (clonedRepos.length === 0) {
    console.log(chalk.yellow('No repositories are cloned. Run `codi-repo init --clone` first.'));
    return;
  }

  // If -b flag, create and checkout
  if (options.create) {
    const { branch } = await import('./branch.js');
    await branch(branchName, { create: true });
    return;
  }

  // Check which repos have the branch
  const branchCheck = await Promise.all(
    clonedRepos.map(async (repo) => ({
      repo,
      exists: await branchExists(repo.absolutePath, branchName),
    }))
  );

  const missing = branchCheck.filter((r) => !r.exists);

  if (missing.length > 0) {
    console.log(
      chalk.yellow(`Branch '${branchName}' doesn't exist in ${missing.length} repos:`)
    );
    for (const { repo } of missing) {
      console.log(chalk.dim(`  - ${repo.name}`));
    }
    console.log('');
    console.log(chalk.dim(`Use 'codi-repo branch ${branchName}' to create it everywhere.`));
    return;
  }

  console.log(
    chalk.blue(`Checking out '${branchName}' in ${clonedRepos.length} repos...\n`)
  );

  const results = await checkoutBranchInAllRepos(clonedRepos, branchName);

  for (const result of results) {
    if (result.success) {
      console.log(chalk.green(`  ✓ ${result.repoName}`));
    } else {
      console.log(chalk.red(`  ✗ ${result.repoName}: ${result.error}`));
    }
  }

  const succeeded = results.filter((r) => r.success).length;
  console.log('');
  console.log(chalk.dim(`Switched ${succeeded}/${clonedRepos.length} repos to ${branchName}`));
}
