# Contributing to codi-repo

Thanks for your interest in contributing! This document outlines how to get started.

## Development Setup

1. Clone the repository:
   ```bash
   git clone git@github.com:laynepenney/codi-repo.git
   cd codi-repo
   ```

2. Install dependencies:
   ```bash
   pnpm install
   ```

3. Build the project:
   ```bash
   pnpm build
   ```

4. Run in development mode:
   ```bash
   pnpm dev <command>
   ```

## Project Structure

```
codi-repo/
├── src/
│   ├── commands/      # CLI command implementations
│   ├── lib/           # Core library functions
│   ├── index.ts       # CLI entry point
│   └── types.ts       # TypeScript type definitions
├── dist/              # Compiled output
├── examples/          # Example manifest files
└── docs/              # Additional documentation
```

## Making Changes

1. Create a branch for your changes:
   ```bash
   git checkout -b feature/your-feature
   ```

2. Make your changes and ensure:
   - Code compiles: `pnpm build`
   - Linting passes: `pnpm lint`
   - Tests pass: `pnpm test`

3. Commit with a descriptive message:
   ```bash
   git commit -m "feat: add new feature"
   ```

   We follow [Conventional Commits](https://www.conventionalcommits.org/):
   - `feat:` - New feature
   - `fix:` - Bug fix
   - `docs:` - Documentation only
   - `refactor:` - Code change that neither fixes a bug nor adds a feature
   - `test:` - Adding or updating tests
   - `chore:` - Maintenance tasks

4. Push and open a pull request.

## Pull Request Guidelines

- Keep PRs focused on a single change
- Update documentation if adding new features
- Add tests for new functionality
- Ensure CI passes before requesting review

## Reporting Issues

When reporting bugs, please include:
- Node.js version (`node --version`)
- Operating system
- Steps to reproduce
- Expected vs actual behavior

## Questions?

Open an issue with the `question` label.
