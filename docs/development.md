# Development environment

How the project is built, checked and protected, and what to set up to
work on it. A pull request that changes tooling, CI, agent settings or
configuration outside the repository updates this page.

## Tools

| Tool | Used for | Version |
|---|---|---|
| [rustup](https://rustup.rs) | Rust toolchain | pinned in `rust-toolchain.toml`: channel, `clippy`, `rustfmt` and the Pi target `aarch64-unknown-linux-musl`; installed on the first `cargo` call |
| [just](https://github.com/casey/just) | command runner, all commands live in `justfile` (`just --list`) | any recent |
| [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) and [Zig](https://ziglang.org) | cross-build for the Pi | any recent |
| [uv](https://github.com/astral-sh/uv) | the Python test CSMS in `csms/` | Python pinned in `csms/.python-version`, dependencies in `csms/uv.lock` |
| [gh](https://cli.github.com) | pull requests and CI status | any recent |

## Setup

Once per clone, before the first commit:

1. Install the tools above.
2. Enable the commit message hook:

   ```sh
   git config core.hooksPath .githooks
   ```

3. Run `just ci`. It installs the pinned toolchain and the CSMS
   dependencies on first use, and it must pass before every push.

Local only, gitignored: `docs/specs/` (OCPP specifications, errata and
JSON schemas, not redistributable; layout in `CLAUDE.md`) and
`.claude/settings.local.json` (personal agent overrides).

## Building for the Pi

`just build-pi` cross-compiles static release binaries for
`aarch64-unknown-linux-musl` with `cargo zigbuild`; Zig is the linker
and ships musl, so no aarch64 GCC is needed. `just deploy` copies the
`cp-sim` binary to the Pi over `scp` (`PI_HOST`, default `cp-sim`, an SSH
host alias). A plain `cargo build` still builds for the laptop. Why:
`docs/decisions/0003-static-musl-build-with-cargo-zigbuild.md`.

## Commit messages

Conventional Commits (`type(scope)!: subject`), checked by
`.githooks/commit-msg` locally and by the `commits` CI job on every pull
request. Rules and reasons: `docs/decisions/0004-commit-message-hook.md`.

## Continuous integration

`.github/workflows/ci.yml` runs on every pull request and on pushes to
`main`:

| Job | Checks | Local |
|---|---|---|
| `rust` | `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo check` for `aarch64-unknown-linux-musl`, `just check-docs` (every test named in the invariants table of `docs/architecture.md` exists) | `just ci-rust` |
| `python` | `ruff check` and `ruff format --check` in `csms/` | `just ci-python` |
| `commits` | every non-merge commit of the PR passes `.githooks/commit-msg` | the hook itself |

The jobs call the same `just` recipes as `just ci`, so local and CI
checks cannot drift. Actions are pinned to commit SHAs. Design and
security choices: `docs/decisions/0006-ci-on-github-actions.md`.

## Coding agent

Instructions for the agent: `CLAUDE.md`.

`.claude/settings.json` is the shared project settings file of
[Claude Code](https://code.claude.com/docs/en/settings):

- **Permissions.** What runs without asking (build, test, the `just`
  check recipes, local git, read-only `gh`), what needs confirmation
  (push, PR creation, dependency changes, deploy, edits to the safety
  files) and what is denied (merging, force-push, skipping the commit
  hook, reading credentials). Rationale and known gaps:
  `docs/decisions/0005-agent-permission-policy.md`.
- **Shell.** `CLAUDE_CODE_SHELL=/bin/bash` in the `env` block makes the
  Bash tool use bash whatever the login shell is. Claude Code supports
  only bash and zsh; wrapping commands in `bash -c` instead would stop
  permission rules from matching the real command.

Personal overrides go into `.claude/settings.local.json`.

### Review subagents

Two [subagents](https://code.claude.com/docs/en/sub-agents) in
`.claude/agents/` review the agent's work before the owner sees it.
`CLAUDE.md` says when they run. Each starts with a fresh context (it
loads `CLAUDE.md`, but not the conversation), so the main agent passes
the plan and the owner's requests in the prompt. Both return numbered
findings (`P1`... and `R1`...) as blocker / should fix / nit and fix
nothing. `plan-reviewer` writes in Russian, since the owner reads its
findings directly; `pr-reviewer` writes in English. A run with a blocker or
should fix is followed by another one, at most three per plan or push
(`CLAUDE.md`, Workflow).

| Subagent | When | Tools | Model, effort |
|---|---|---|---|
| `plan-reviewer` | before a plan is presented | `Read`, `Grep`, `Glob` | `opus`, `medium` |
| `pr-reviewer` | before every push | `Read`, `Grep`, `Glob`, `Bash` behind a hook | `sonnet`, `medium` |

- **Model and effort** are set in the frontmatter so they do not follow
  the main session. A `model` or `effort` passed with an invocation, and
  the environment variables `CLAUDE_CODE_SUBAGENT_MODEL` (only when the
  field is missing) and `CLAUDE_CODE_EFFORT_LEVEL` (always), would
  override them; `CLAUDE.md` forbids the first, the variables are not
  set in this project.
- **Bash for `pr-reviewer`.** The `tools` field cannot allow only some
  Bash commands, so a PreToolUse hook declared in the agent's
  frontmatter, `.claude/hooks/pr-reviewer-bash.sh`, checks every command
  while the subagent runs. It is an allowlist: read-only `git` and `gh`,
  `cargo` build, check, test and clippy without `--fix`, `cargo fmt
  --check`, the `just` check recipes and a few read-only text tools. It
  blocks output redirection, command substitution, background jobs,
  paths to credentials (`.env`, `.ssh`, `*.pem`, ...), options that run
  helper programs (`git --ext-diff`, `cargo --config`), and anything it
  cannot parse (e.g. a quoted `|`). It needs `jq`. Text matching cannot
  see through quotes and globs (`cat .e""nv`, `cat ~/.ss?/id_rsa`) or
  stop reads outside the repository; the Bash sandbox in the roadmap
  backlog is what would close that.
- **Hooks need workspace trust.** Frontmatter hooks of project subagents
  run only after the folder is trusted, and never in `claude -p`. In an
  untrusted session `pr-reviewer` has unguarded Bash, still limited by
  the permission rules above.
- **Per-commit check.** `just test-commits base` exports each commit
  since `base` with `git archive` into a temporary directory and runs
  `cargo test --locked --workspace` there; the build cache lives in
  `target/test-commits`. `pr-reviewer` runs it, and so can anyone.
- **Revert spot check.** `pr-reviewer` cannot change files, so it names
  the lines to revert and the test that should fail; the main agent runs
  that check.

## Configured outside the repository

GitHub settings that exist only in the web UI. Nothing here is visible in
the code, so recreate them by hand from this checklist, for example after
moving the repository.

**Ruleset on `main`** (Settings → Rules → Rulesets):

- [ ] Target: `main`.
- [ ] Require a pull request before merging, 0 required approvals.
- [ ] Require status checks to pass: `rust`, `python`, `commits`.
- [ ] Block force pushes.
- [ ] Restrict deletions.

**Merge method** (Settings → General → Pull Requests):

- [ ] Allow merge commits only; squash and rebase merging off, so the
      history keeps every commit of a pull request with its hash.

**Advanced Security** (Settings → Advanced Security):

- [ ] Secret scanning on, with push protection.
- [ ] Dependency graph on.
- [ ] Dependabot alerts on.
- [ ] Dependabot updates off (planned as `.github/dependabot.yml`, see
      the roadmap backlog).
- [ ] AI features off.

**Agent access**:

- [ ] Fine-grained personal access token, repository access limited to
      this repository only.
- [ ] Repository permissions: Contents read and write, Pull requests read
      and write, Workflows read and write, Actions read, Metadata read.
- [ ] Expiry 90 days; create a new token before it expires.
- [ ] Stored by `gh` in the system keyring (`gh auth login` with the
      token), not in a file or an environment variable.
