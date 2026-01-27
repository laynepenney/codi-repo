import chalk from 'chalk';
import ora from 'ora';
import inquirer from 'inquirer';
import { loadManifest, getAllRepoInfo, loadState, saveState, parseGitHubUrl } from '../../lib/manifest.js';
import {
  pathExists,
  getCurrentBranch,
  hasCommitsAhead,
  pushBranch,
  remoteBranchExists,
} from '../../lib/git.js';
import {
  createPullRequest,
  createLinkedPRs,
  generateManifestPRBody,
  findPRByBranch,
} from '../../lib/github.js';
import { linkBranchToManifestPR, saveLinkedPRs } from '../../lib/linker.js';
import type { RepoInfo, PRCreateOptions } from '../../types.js';

interface CreateOptions {
  title?: string;
  body?: string;
  draft?: boolean;
  base?: string;
  push?: boolean;
}

/**
 * Create linked PRs across all repositories with changes
 */
export async function createPR(options: CreateOptions = {}): Promise<void> {
  const { manifest, rootDir } = await loadManifest();
  const repos = getAllRepoInfo(manifest, rootDir);

  // Check which repos are cloned
  const clonedRepos: RepoInfo[] = [];
  for (const repo of repos) {
    if (await pathExists(repo.absolutePath)) {
      clonedRepos.push(repo);
    }
  }

  if (clonedRepos.length === 0) {
    console.log(chalk.yellow('No repositories are cloned.'));
    return;
  }

  // Get current branch
  const branches = await Promise.all(
    clonedRepos.map(async (repo) => ({
      repo,
      branch: await getCurrentBranch(repo.absolutePath),
    }))
  );

  // Check all repos are on the same branch
  const uniqueBranches = [...new Set(branches.map((b) => b.branch))];
  if (uniqueBranches.length > 1) {
    console.log(chalk.yellow('Repositories are on different branches:'));
    for (const { repo, branch } of branches) {
      console.log(`  ${repo.name}: ${chalk.cyan(branch)}`);
    }
    console.log('');
    console.log(chalk.dim('Use `codi-repo checkout <branch>` to sync branches first.'));
    return;
  }

  const branchName = uniqueBranches[0];

  // Check it's not the default branch
  const onDefaultBranch = clonedRepos.some((repo) => repo.default_branch === branchName);
  if (onDefaultBranch) {
    console.log(chalk.yellow(`You're on the default branch (${branchName}).`));
    console.log(chalk.dim('Create a feature branch first with `codi-repo branch <name>`.'));
    return;
  }

  console.log(chalk.blue(`Creating PRs for branch: ${chalk.cyan(branchName)}\n`));

  // Check which repos have commits to push
  const reposWithChanges: { repo: RepoInfo; hasChanges: boolean; needsPush: boolean }[] =
    await Promise.all(
      clonedRepos.map(async (repo) => {
        const hasChanges = await hasCommitsAhead(repo.absolutePath, repo.default_branch);
        const needsPush = hasChanges && !(await remoteBranchExists(repo.absolutePath, branchName));
        return { repo, hasChanges, needsPush };
      })
    );

  const withChanges = reposWithChanges.filter((r) => r.hasChanges);

  if (withChanges.length === 0) {
    console.log(chalk.yellow('No repositories have commits ahead of their default branch.'));
    console.log(chalk.dim('Make some commits first, then run this command again.'));
    return;
  }

  console.log(`Found changes in ${withChanges.length} repos:`);
  for (const { repo } of withChanges) {
    console.log(`  ${chalk.green('•')} ${repo.name}`);
  }
  console.log('');

  // Check if any need to be pushed first
  const needsPush = reposWithChanges.filter((r) => r.needsPush);
  if (needsPush.length > 0) {
    if (options.push) {
      console.log(chalk.dim('Pushing branches to remote...\n'));
      for (const { repo } of needsPush) {
        const spinner = ora(`Pushing ${repo.name}...`).start();
        try {
          await pushBranch(repo.absolutePath, branchName, 'origin', true);
          spinner.succeed(`${repo.name}: pushed`);
        } catch (error) {
          spinner.fail(`${repo.name}: ${error instanceof Error ? error.message : error}`);
          console.log(chalk.red('\nFailed to push. Fix the error and try again.'));
          return;
        }
      }
      console.log('');
    } else {
      console.log(chalk.yellow('Some branches need to be pushed to remote first:'));
      for (const { repo } of needsPush) {
        console.log(`  ${repo.name}`);
      }
      console.log('');
      console.log(chalk.dim('Run with --push flag to push automatically, or push manually.'));
      return;
    }
  }

  // Get PR title if not provided
  let title: string = options.title ?? '';
  if (!title) {
    const answers = await inquirer.prompt([
      {
        type: 'input',
        name: 'title',
        message: 'PR title:',
        default: branchName.replace(/[-_]/g, ' ').replace(/^feature\//, ''),
        validate: (input: string) => input.length > 0 || 'Title is required',
      },
    ]);
    title = answers.title as string;
  }

  // Get PR body if not provided
  let body = options.body ?? '';
  if (!body) {
    const answers = await inquirer.prompt([
      {
        type: 'editor',
        name: 'body',
        message: 'PR description (optional):',
        default: '',
      },
    ]);
    body = answers.body.trim();
  }

  // Create PRs
  const spinner = ora('Creating pull requests...').start();

  try {
    // First, check if manifest PR already exists
    // For now, we'll use the first repo with a manifest setting or skip manifest PR
    // In a real setup, you'd have a manifest repo defined in settings

    const reposForPR = withChanges.map((r) => r.repo);
    const prOptions: PRCreateOptions = {
      title,
      body,
      draft: options.draft,
      base: options.base,
    };

    // Create PRs in each repo (without manifest reference for now)
    const linkedPRs = await createLinkedPRs(reposForPR, branchName, prOptions);

    spinner.succeed('Pull requests created!\n');

    // Display results
    console.log(chalk.green('Created PRs:'));
    for (const pr of linkedPRs) {
      console.log(`  ${pr.repoName}: ${chalk.cyan(pr.url)}`);
    }

    // Generate a summary for the user
    console.log('');
    console.log(chalk.dim('To view PR status: codi-repo pr status'));
    console.log(chalk.dim('To merge all PRs:  codi-repo pr merge'));

    // Save state
    const state = await loadState(rootDir);
    // We don't have a manifest PR number in simple mode, use branch name as key
    state.branchToPR[branchName] = -1; // Placeholder
    await saveState(rootDir, state);
  } catch (error) {
    spinner.fail('Failed to create PRs');
    console.error(chalk.red(error instanceof Error ? error.message : String(error)));
  }
}
