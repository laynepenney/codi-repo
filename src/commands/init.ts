import { writeFile, mkdir, access } from 'fs/promises';
import { resolve } from 'path';
import chalk from 'chalk';
import ora from 'ora';
import inquirer from 'inquirer';
import {
  loadManifest,
  createManifest,
  generateSampleManifest,
  getAllRepoInfo,
} from '../lib/manifest.js';
import { cloneRepo, pathExists } from '../lib/git.js';
import type { Manifest } from '../types.js';

interface InitOptions {
  clone?: boolean;
  force?: boolean;
}

/**
 * Initialize a new codi-repo workspace
 */
export async function init(options: InitOptions = {}): Promise<void> {
  const cwd = process.cwd();
  const manifestPath = resolve(cwd, 'codi-repos.yaml');

  // Check if manifest already exists
  let manifest: Manifest;
  let rootDir: string;

  try {
    await access(manifestPath);
    // Manifest exists
    if (!options.force) {
      console.log(chalk.yellow('Manifest file already exists.'));
      const { action } = await inquirer.prompt([
        {
          type: 'list',
          name: 'action',
          message: 'What would you like to do?',
          choices: [
            { name: 'Clone missing repositories', value: 'clone' },
            { name: 'Overwrite manifest with sample', value: 'overwrite' },
            { name: 'Cancel', value: 'cancel' },
          ],
        },
      ]);

      if (action === 'cancel') {
        console.log('Cancelled.');
        return;
      }

      if (action === 'overwrite') {
        manifest = generateSampleManifest();
        await createManifest(cwd, manifest);
        console.log(chalk.green('Created sample manifest at codi-repos.yaml'));
        console.log(chalk.dim('Edit the manifest to configure your repositories, then run:'));
        console.log(chalk.cyan('  codi-repo init --clone'));
        return;
      }
    }

    // Load existing manifest
    const loaded = await loadManifest(manifestPath);
    manifest = loaded.manifest;
    rootDir = loaded.rootDir;
  } catch {
    // No manifest exists, create sample
    manifest = generateSampleManifest();
    await createManifest(cwd, manifest);
    rootDir = cwd;

    console.log(chalk.green('Created sample manifest at codi-repos.yaml'));
    console.log('');
    console.log(chalk.dim('Edit the manifest to configure your repositories:'));
    console.log('');
    console.log(chalk.cyan(`  repos:
    public:
      url: git@github.com:your-org/your-repo.git
      path: ./public
      default_branch: main`));
    console.log('');
    console.log(chalk.dim('Then run:'));
    console.log(chalk.cyan('  codi-repo init --clone'));
    return;
  }

  // Clone repositories if requested
  if (options.clone) {
    const repos = getAllRepoInfo(manifest, rootDir);
    console.log(chalk.blue(`Found ${repos.length} repositories in manifest\n`));

    for (const repo of repos) {
      const exists = await pathExists(repo.absolutePath);

      if (exists) {
        console.log(chalk.dim(`  ${repo.name}: already exists at ${repo.path}`));
        continue;
      }

      const spinner = ora(`Cloning ${repo.name}...`).start();
      try {
        await cloneRepo(repo.url, repo.absolutePath, repo.default_branch);
        spinner.succeed(`Cloned ${repo.name} to ${repo.path}`);
      } catch (error) {
        spinner.fail(`Failed to clone ${repo.name}`);
        console.error(chalk.red(`  Error: ${error instanceof Error ? error.message : error}`));
      }
    }

    console.log('');
    console.log(chalk.green('Initialization complete!'));
    console.log(chalk.dim('Run `codi-repo status` to see the status of all repositories.'));
  }

  // Create .codi-repo directory for state
  const stateDir = resolve(rootDir, '.codi-repo');
  try {
    await mkdir(stateDir, { recursive: true });
  } catch {
    // Directory might already exist
  }

  // Add .codi-repo to .gitignore if it exists
  const gitignorePath = resolve(rootDir, '.gitignore');
  try {
    await access(gitignorePath);
    const { appendFile, readFile } = await import('fs/promises');
    const content = await readFile(gitignorePath, 'utf-8');
    if (!content.includes('.codi-repo')) {
      await appendFile(gitignorePath, '\n# codi-repo state\n.codi-repo/\n');
      console.log(chalk.dim('Added .codi-repo/ to .gitignore'));
    }
  } catch {
    // No .gitignore, create one
    await writeFile(gitignorePath, '# codi-repo state\n.codi-repo/\n');
    console.log(chalk.dim('Created .gitignore with .codi-repo/'));
  }
}
