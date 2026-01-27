import { describe, it, expect } from 'vitest';
import { parseGitHubUrl, generateSampleManifest } from '../manifest.js';

describe('parseGitHubUrl', () => {
  it('parses SSH URLs', () => {
    const result = parseGitHubUrl('git@github.com:owner/repo.git');
    expect(result).toEqual({ owner: 'owner', repo: 'repo' });
  });

  it('parses SSH URLs without .git suffix', () => {
    const result = parseGitHubUrl('git@github.com:owner/repo');
    expect(result).toEqual({ owner: 'owner', repo: 'repo' });
  });

  it('parses HTTPS URLs', () => {
    const result = parseGitHubUrl('https://github.com/owner/repo.git');
    expect(result).toEqual({ owner: 'owner', repo: 'repo' });
  });

  it('parses HTTPS URLs without .git suffix', () => {
    const result = parseGitHubUrl('https://github.com/owner/repo');
    expect(result).toEqual({ owner: 'owner', repo: 'repo' });
  });

  it('throws on invalid URLs', () => {
    expect(() => parseGitHubUrl('not-a-url')).toThrow();
    expect(() => parseGitHubUrl('https://gitlab.com/owner/repo')).toThrow();
  });
});

describe('generateSampleManifest', () => {
  it('returns a valid manifest structure', () => {
    const manifest = generateSampleManifest();

    expect(manifest.version).toBe(1);
    expect(manifest.repos).toBeDefined();
    expect(Object.keys(manifest.repos).length).toBeGreaterThan(0);
    expect(manifest.settings).toBeDefined();
    expect(manifest.settings.pr_prefix).toBe('[cross-repo]');
    expect(manifest.settings.merge_strategy).toBe('all-or-nothing');
  });

  it('includes required fields for each repo', () => {
    const manifest = generateSampleManifest();

    for (const [name, repo] of Object.entries(manifest.repos)) {
      expect(repo.url).toBeDefined();
      expect(repo.path).toBeDefined();
      expect(repo.default_branch).toBeDefined();
    }
  });
});
