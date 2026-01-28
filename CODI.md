# codi-repo Development Guide

Multi-repository orchestration CLI for unified PR workflows.

## Build & Test

```bash
pnpm install          # Install dependencies
pnpm build            # Compile TypeScript
pnpm test             # Run tests
pnpm lint             # Lint code
```

## Project Structure

```
src/
├── index.ts              # CLI entry point (Commander.js)
├── types.ts              # TypeScript interfaces
├── commands/             # CLI command implementations
│   ├── init.ts           # cr init
│   ├── sync.ts           # cr sync
│   ├── status.ts         # cr status
│   ├── branch.ts         # cr branch
│   ├── checkout.ts       # cr checkout
│   ├── link.ts           # cr link
│   ├── run.ts            # cr run
│   ├── env.ts            # cr env
│   └── pr/               # PR subcommands
│       ├── index.ts
│       ├── create.ts
│       ├── status.ts
│       └── merge.ts
└── lib/                  # Core libraries
    ├── manifest.ts       # Manifest parsing & validation
    ├── git.ts            # Git operations
    ├── github.ts         # GitHub CLI wrapper
    ├── files.ts          # copyfile/linkfile operations
    ├── hooks.ts          # Post-sync/checkout hooks
    └── scripts.ts        # Workspace script runner
```

## Key Concepts

### Manifest
Workspace configuration in `.codi-repo/manifests/manifest.yaml`:
- `repos`: Repository definitions with URL, path, default_branch
- `manifest`: Self-tracking for the manifest repo itself
- `workspace`: Scripts, hooks, and environment variables
- `settings`: PR prefix, merge strategy

### Commands
All commands use `cr` alias:
- `cr init <url>` - Initialize workspace from manifest
- `cr sync` - Pull all repos + process links + run hooks
- `cr status` - Show repo status
- `cr branch/checkout` - Branch operations across all repos
- `cr pr create/status/merge` - Linked PR workflow
- `cr link` - Manage copyfile/linkfile entries
- `cr run` - Execute workspace scripts

### File Linking
- `copyfile`: Copy file from repo to workspace
- `linkfile`: Create symlink from workspace to repo
- Path validation prevents directory traversal

## Testing

```bash
pnpm test              # Run all tests
pnpm test:watch        # Watch mode
pnpm test -- --grep "manifest"  # Filter tests
```

Test files are in `src/lib/__tests__/`.

## Adding a New Command

1. Create `src/commands/mycommand.ts`
2. Export the handler function
3. Register in `src/index.ts` with Commander
4. Add types to `src/types.ts` if needed

## Code Style

- TypeScript strict mode
- Async/await for all I/O
- Use `chalk` for colored output
- Use `ora` for spinners
- Validate manifest schema in `lib/manifest.ts`
