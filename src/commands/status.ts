import chalk from 'chalk';
import { loadManifest, getAllRepoInfo } from '../lib/manifest.js';
import { getAllRepoStatus } from '../lib/git.js';
import type { RepoStatus } from '../types.js';

interface StatusOptions {
  json?: boolean;
}

/**
 * Format a single repo status line
 */
function formatRepoStatus(status: RepoStatus): string {
  if (!status.exists) {
    return `${chalk.yellow(status.name)}: ${chalk.dim('not cloned')}`;
  }

  const parts: string[] = [];

  // Branch name
  parts.push(chalk.cyan(status.branch));

  // Clean/dirty indicator
  if (status.clean) {
    parts.push(chalk.green('clean'));
  } else {
    const changes: string[] = [];
    if (status.staged > 0) {
      changes.push(chalk.green(`+${status.staged} staged`));
    }
    if (status.modified > 0) {
      changes.push(chalk.yellow(`~${status.modified} modified`));
    }
    if (status.untracked > 0) {
      changes.push(chalk.dim(`?${status.untracked} untracked`));
    }
    parts.push(changes.join(', '));
  }

  // Ahead/behind
  if (status.ahead > 0 || status.behind > 0) {
    const sync: string[] = [];
    if (status.ahead > 0) {
      sync.push(chalk.green(`↑${status.ahead}`));
    }
    if (status.behind > 0) {
      sync.push(chalk.red(`↓${status.behind}`));
    }
    parts.push(sync.join(' '));
  }

  return `${chalk.bold(status.name)}: ${parts.join(' | ')}`;
}

/**
 * Show status of all repositories
 */
export async function status(options: StatusOptions = {}): Promise<void> {
  const { manifest, rootDir } = await loadManifest();
  const repos = getAllRepoInfo(manifest, rootDir);
  const statuses = await getAllRepoStatus(repos);

  if (options.json) {
    console.log(JSON.stringify(statuses, null, 2));
    return;
  }

  console.log(chalk.blue('Repository Status\n'));

  // Find longest repo name for alignment
  const maxNameLength = Math.max(...statuses.map((s) => s.name.length));

  for (const status of statuses) {
    const paddedName = status.name.padEnd(maxNameLength);

    if (!status.exists) {
      console.log(`  ${chalk.yellow(paddedName)}  ${chalk.dim('not cloned')}`);
      continue;
    }

    const parts: string[] = [];

    // Branch with fixed width
    const branchDisplay = status.branch.length > 20
      ? status.branch.slice(0, 17) + '...'
      : status.branch.padEnd(20);
    parts.push(chalk.cyan(branchDisplay));

    // Status indicators
    if (status.clean) {
      parts.push(chalk.green('✓'));
    } else {
      const indicators: string[] = [];
      if (status.staged > 0) indicators.push(chalk.green(`+${status.staged}`));
      if (status.modified > 0) indicators.push(chalk.yellow(`~${status.modified}`));
      if (status.untracked > 0) indicators.push(chalk.dim(`?${status.untracked}`));
      parts.push(indicators.join(' '));
    }

    // Ahead/behind
    if (status.ahead > 0 || status.behind > 0) {
      const sync: string[] = [];
      if (status.ahead > 0) sync.push(chalk.green(`↑${status.ahead}`));
      if (status.behind > 0) sync.push(chalk.red(`↓${status.behind}`));
      parts.push(sync.join(' '));
    }

    console.log(`  ${chalk.bold(paddedName)}  ${parts.join('  ')}`);
  }

  // Summary
  console.log('');
  const cloned = statuses.filter((s) => s.exists).length;
  const dirty = statuses.filter((s) => s.exists && !s.clean).length;
  const notCloned = statuses.filter((s) => !s.exists).length;

  const summaryParts: string[] = [];
  summaryParts.push(`${cloned}/${repos.length} cloned`);
  if (dirty > 0) {
    summaryParts.push(chalk.yellow(`${dirty} with changes`));
  }
  if (notCloned > 0) {
    summaryParts.push(chalk.dim(`${notCloned} not cloned`));
  }

  console.log(chalk.dim(`  ${summaryParts.join(' | ')}`));

  // Check if all on same branch
  const branches = new Set(statuses.filter((s) => s.exists).map((s) => s.branch));
  if (branches.size > 1) {
    console.log('');
    console.log(chalk.yellow('  ⚠ Repositories are on different branches'));
  }
}
