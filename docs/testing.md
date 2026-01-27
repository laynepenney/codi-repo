# Testing codi-repo

## Unit Tests (Mocked)

Run all unit tests:
```bash
npm test
```

These tests mock the GitHub API and git operations, so they don't require any external access.

## E2E Tests (Real GitHub API)

To run tests against the real GitHub API:

```bash
# Set up test environment
export TEST_GITHUB_OWNER=your-username
export TEST_GITHUB_REPO=your-test-repo

# Run E2E tests
GITHUB_E2E=1 npx vitest run src/lib/__tests__/github.e2e.test.ts
```

Requirements:
- `gh` CLI authenticated (`gh auth login`), OR
- `GITHUB_TOKEN` environment variable set
- Access to the test repository

## Manual Testing

### 1. Set Up Test Workspace

```bash
# Create a test directory
mkdir ~/test-codi-repo && cd ~/test-codi-repo

# Initialize (creates sample manifest)
npx codi-repo init

# Edit the manifest to point to your test repos
vim codi-repos.yaml
```

Example manifest for testing:
```yaml
version: 1
repos:
  test-public:
    url: git@github.com:your-username/test-public.git
    path: ./test-public
    default_branch: main
  test-private:
    url: git@github.com:your-username/test-private.git
    path: ./test-private
    default_branch: main
settings:
  pr_prefix: "[cross-repo]"
  merge_strategy: all-or-nothing
```

### 2. Clone Repositories

```bash
npx codi-repo init --clone
```

### 3. Test Branch Operations

```bash
# Check status
npx codi-repo status

# Create a feature branch in all repos
npx codi-repo branch feature/test-1

# Make changes in each repo
echo "test" >> test-public/test.txt
echo "test" >> test-private/test.txt

# Commit changes
cd test-public && git add . && git commit -m "Test change" && cd ..
cd test-private && git add . && git commit -m "Test change" && cd ..

# Check status again
npx codi-repo status
```

### 4. Test PR Creation

```bash
# Create linked PRs (will push branches first)
npx codi-repo pr create --push --title "Test cross-repo PR"

# Check PR status
npx codi-repo pr status
```

### 5. Test PR Merge

```bash
# After PRs are approved
npx codi-repo pr merge
```

## Creating Test Repositories

For thorough testing, create 2-3 test repositories on GitHub:

1. Go to github.com and create new repos:
   - `test-codi-public` (public)
   - `test-codi-private` (private)

2. Add a simple file to each:
   ```bash
   echo "# Test Repo" > README.md
   git add . && git commit -m "Initial commit"
   git push origin main
   ```

3. Update your manifest to use these repos

## Debugging

Enable verbose output:
```bash
DEBUG=* npx codi-repo status
```

Check GitHub token:
```bash
gh auth status
gh auth token
```

Test GitHub API directly:
```bash
gh api repos/owner/repo/pulls
```
