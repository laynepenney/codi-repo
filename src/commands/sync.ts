import chalk from 'chalk';
import ora from 'ora';
import { loadManifest, getAllRepoInfo } from '../lib/manifest.js';
import { pullLatest, fetchRemote, pathExists, getCurrentBranch } from '../lib/git.js';
import type { RepoInfo } from '../types.js';

interface SyncOptions {
  fetch?: boolean;
  all?: boolean;
}

/**
 * Sync (pull or fetch) all repositories
 */
export async function sync(options: SyncOptions = {}): Promise<void> {
  const { manifest, rootDir } = await loadManifest();
  const repos = getAllRepoInfo(manifest, rootDir);

  console.log(chalk.blue(`Syncing ${repos.length} repositories...\n`));

  const results: { repo: RepoInfo; success: boolean; error?: string; branch?: string }[] = [];

  for (const repo of repos) {
    const exists = await pathExists(repo.absolutePath);

    if (!exists) {
      console.log(chalk.yellow(`  ${repo.name}: not cloned (run 'codi-repo init --clone')`));
      results.push({ repo, success: false, error: 'not cloned' });
      continue;
    }

    const spinner = ora(`${options.fetch ? 'Fetching' : 'Pulling'} ${repo.name}...`).start();

    try {
      const branch = await getCurrentBranch(repo.absolutePath);

      if (options.fetch) {
        await fetchRemote(repo.absolutePath);
        spinner.succeed(`${repo.name} (${chalk.cyan(branch)}): fetched`);
      } else {
        await pullLatest(repo.absolutePath);
        spinner.succeed(`${repo.name} (${chalk.cyan(branch)}): pulled`);
      }

      results.push({ repo, success: true, branch });
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);

      // Check for common errors
      if (errorMsg.includes('uncommitted changes')) {
        spinner.warn(`${repo.name}: has uncommitted changes, skipping`);
        results.push({ repo, success: false, error: 'uncommitted changes' });
      } else if (errorMsg.includes('diverged')) {
        spinner.warn(`${repo.name}: branch has diverged from remote`);
        results.push({ repo, success: false, error: 'diverged' });
      } else {
        spinner.fail(`${repo.name}: ${errorMsg}`);
        results.push({ repo, success: false, error: errorMsg });
      }
    }
  }

  // Summary
  console.log('');
  const succeeded = results.filter((r) => r.success).length;
  const failed = results.filter((r) => !r.success).length;

  if (failed === 0) {
    console.log(chalk.green(`All ${succeeded} repositories synced successfully.`));
  } else {
    console.log(
      chalk.yellow(`Synced ${succeeded}/${repos.length} repositories. ${failed} had issues.`)
    );
  }
}
