import { readFile, writeFile, access } from 'fs/promises';
import { resolve, dirname } from 'path';
import YAML from 'yaml';
import type { Manifest, RepoInfo, StateFile, GitHubRepoInfo } from '../types.js';

const MANIFEST_FILENAME = 'codi-repos.yaml';
const STATE_DIR = '.codi-repo';
const STATE_FILENAME = 'state.json';

/**
 * Default manifest settings
 */
const DEFAULT_SETTINGS = {
  pr_prefix: '[cross-repo]',
  merge_strategy: 'all-or-nothing' as const,
};

/**
 * Find the manifest file by walking up the directory tree
 */
export async function findManifestPath(startDir: string = process.cwd()): Promise<string | null> {
  let currentDir = resolve(startDir);
  const root = dirname(currentDir);

  while (currentDir !== root) {
    const manifestPath = resolve(currentDir, MANIFEST_FILENAME);
    try {
      await access(manifestPath);
      return manifestPath;
    } catch {
      currentDir = dirname(currentDir);
    }
  }

  // Check root as well
  const rootManifest = resolve(root, MANIFEST_FILENAME);
  try {
    await access(rootManifest);
    return rootManifest;
  } catch {
    return null;
  }
}

/**
 * Load and parse the manifest file
 */
export async function loadManifest(manifestPath?: string): Promise<{ manifest: Manifest; rootDir: string }> {
  const path = manifestPath ?? (await findManifestPath());
  if (!path) {
    throw new Error(`Manifest file not found. Run 'codi-repo init' first or create ${MANIFEST_FILENAME}`);
  }

  const content = await readFile(path, 'utf-8');
  const parsed = YAML.parse(content) as Partial<Manifest>;

  // Validate and apply defaults
  if (!parsed.version) {
    parsed.version = 1;
  }
  if (!parsed.repos || Object.keys(parsed.repos).length === 0) {
    throw new Error('Manifest must define at least one repository');
  }
  if (!parsed.settings) {
    parsed.settings = DEFAULT_SETTINGS;
  } else {
    parsed.settings = { ...DEFAULT_SETTINGS, ...parsed.settings };
  }

  // Validate each repo config
  for (const [name, repo] of Object.entries(parsed.repos)) {
    if (!repo.url) {
      throw new Error(`Repository '${name}' is missing 'url'`);
    }
    if (!repo.path) {
      throw new Error(`Repository '${name}' is missing 'path'`);
    }
    if (!repo.default_branch) {
      repo.default_branch = 'main';
    }
  }

  return {
    manifest: parsed as Manifest,
    rootDir: dirname(path),
  };
}

/**
 * Create a new manifest file
 */
export async function createManifest(rootDir: string, manifest: Manifest): Promise<void> {
  const manifestPath = resolve(rootDir, MANIFEST_FILENAME);
  const content = YAML.stringify(manifest, {
    indent: 2,
    lineWidth: 0,
  });
  await writeFile(manifestPath, content, 'utf-8');
}

/**
 * Parse GitHub owner/repo from a git URL
 */
export function parseGitHubUrl(url: string): GitHubRepoInfo {
  // SSH format: git@github.com:owner/repo.git
  const sshMatch = url.match(/git@github\.com:([^/]+)\/(.+?)(?:\.git)?$/);
  if (sshMatch) {
    return { owner: sshMatch[1], repo: sshMatch[2] };
  }

  // HTTPS format: https://github.com/owner/repo.git
  const httpsMatch = url.match(/https?:\/\/github\.com\/([^/]+)\/(.+?)(?:\.git)?$/);
  if (httpsMatch) {
    return { owner: httpsMatch[1], repo: httpsMatch[2] };
  }

  throw new Error(`Unable to parse GitHub URL: ${url}`);
}

/**
 * Get full repo info with computed fields
 */
export function getRepoInfo(name: string, config: Manifest['repos'][string], rootDir: string): RepoInfo {
  const { owner, repo } = parseGitHubUrl(config.url);
  return {
    ...config,
    name,
    absolutePath: resolve(rootDir, config.path),
    owner,
    repo,
  };
}

/**
 * Get all repos as RepoInfo array
 */
export function getAllRepoInfo(manifest: Manifest, rootDir: string): RepoInfo[] {
  return Object.entries(manifest.repos).map(([name, config]) => getRepoInfo(name, config, rootDir));
}

/**
 * Get the state file path
 */
function getStatePath(rootDir: string): string {
  return resolve(rootDir, STATE_DIR, STATE_FILENAME);
}

/**
 * Load the state file
 */
export async function loadState(rootDir: string): Promise<StateFile> {
  const statePath = getStatePath(rootDir);
  try {
    const content = await readFile(statePath, 'utf-8');
    return JSON.parse(content) as StateFile;
  } catch {
    return {
      branchToPR: {},
      prLinks: {},
    };
  }
}

/**
 * Save the state file
 */
export async function saveState(rootDir: string, state: StateFile): Promise<void> {
  const statePath = getStatePath(rootDir);
  const stateDir = dirname(statePath);

  // Ensure state directory exists
  const { mkdir } = await import('fs/promises');
  await mkdir(stateDir, { recursive: true });

  await writeFile(statePath, JSON.stringify(state, null, 2), 'utf-8');
}

/**
 * Generate a sample manifest for init command
 */
export function generateSampleManifest(): Manifest {
  return {
    version: 1,
    repos: {
      public: {
        url: 'git@github.com:your-org/your-repo.git',
        path: './public',
        default_branch: 'main',
      },
      private: {
        url: 'git@github.com:your-org/your-private-repo.git',
        path: './private',
        default_branch: 'main',
      },
    },
    settings: {
      pr_prefix: '[cross-repo]',
      merge_strategy: 'all-or-nothing',
    },
  };
}
