# 0005. Permission policy for the coding agent

## Context

The repository is public and `main` is protected by a ruleset: changes
land only through pull requests. A coding agent (Claude Code, in the
terminal or in the Code tab of Claude Desktop) works in the checkout with
a GitHub token and can run shell commands, so its reach needs limits that
do not depend on the agent behaving well.

Claude Code evaluates permission rules in the order deny, ask, allow, and
the first match wins, across all settings files. A deny rule cannot be
relaxed by a narrower allow rule. Rules are enforced by Claude Code, not
by the model. Allow rules from a committed `.claude/settings.json` apply
only after the folder is trusted; ask and deny rules apply immediately.

## Decision

Commit a project policy in `.claude/settings.json`; personal overrides
stay in the gitignored `.claude/settings.local.json`.

- **Allow**: reading and editing files in the repository; `cargo build`,
  `check`, `test`, `clippy`, `fmt`; `just test`, `build-pi`, `csms`;
  local git (`status`, `diff`, `log`, `show`, `add`, `commit`, `branch`,
  `switch`, `checkout -b`); read-only `gh` (`pr view|checks|list`,
  `run view|list`); web fetch from docs.rs, doc.rust-lang.org,
  docs.github.com and code.claude.com.
- **Ask**: `git push`, `gh pr create`, `cargo add|remove|install`,
  edits to `Cargo.toml` and `csms/pyproject.toml`, `rustup`,
  `uv tool install`, `uv add`, `just deploy`, `scp`, `ssh`, `curl`,
  `wget`, `git reset --hard`, `git clean`, edits under `.claude/`, and
  edits to `justfile`, `.githooks/**` and `build.rs` (code that allowed
  commands and hooks execute).
- **Deny**: `gh pr merge`, force pushes, `git commit --no-verify` and
  `-n` (the forms `-n...`, `-an...` and `... -n...`), `git config`,
  `gh auth|secret|repo edit|repo delete|api`, reading `~/.ssh`,
  `~/.config/gh`, `~/.aws`, `.env*`, `*.pem`, `*.key`, `sudo`, edits in
  system directories.

Defence in depth, from the outside in:

1. **Fine-grained GitHub token** scoped to this repository: even a fully
   compromised session cannot touch other repositories or the account.
2. **Ruleset on `main`**: nothing reaches `main` without a pull request
   and a human merge, whatever the client rules say.
3. **Client-side rules** (this file): stop mistakes and routine
   overreach early and make risky steps visible as prompts.
4. **Commit message hook** (ADR 0004) and review of every PR.

## Consequences

- Day-to-day work (edit, build, test, commit) does not prompt; anything
  that leaves the machine or changes dependencies does.
- The client rules are a convenience layer, not a security boundary. The
  token scope and the ruleset are the boundary.

Known limitations:

- **Bash rules match command text.** `/usr/bin/scp`, `sh -c '...'`,
  `git -C . push` or `git -c ... push` are not matched by rules written
  for `scp`, `ssh`, `curl`, `wget` or `git push`. Short combined flags
  (`git push -fu`) and `+refspec` force pushes are not caught by the
  force-push denies.
- **`git commit -n` is only partly covered.** Clusters that start with
  `n` or `an`, and a standalone `-n` after other arguments, are denied.
  Other clusters (`-vn`, `-sn`) are not, and a message that contains
  ` -n` is denied as a false positive.
- **Allowed commands run repository code.** `cargo test`/`build` execute
  tests and build scripts, `just test`/`csms` run whatever the
  `justfile` says. Editing `justfile`, `.githooks/` and `build.rs` asks
  first, but other code they run (for example proc-macro crates, test
  files or the Python CSMS) is not gated.
- **Read and edit denies cover Claude's file tools and the Bash file
  commands Claude Code recognises**, not arbitrary processes (a script
  that opens a file itself).
- **"Edits outside the repository are denied" is approximated.** A deny
  rule cannot be narrowed by an allow rule, so "everything except the
  repo" cannot be written. Edits outside the working directory prompt by
  default; the deny list only hardens well-known system directories.
- **Web fetch from other domains is not an ask rule.** An ask rule for
  all domains would also match the allowed ones (ask is evaluated before
  allow). Other domains prompt by default instead. `curl` and `wget`
  ask, but they bypass the domain rules once approved, and other
  network tools (`rsync`, `sftp`, `nc`, scripting languages) are not
  gated.
- **Allow rules wait for workspace trust.** An allow rule does not match
  past a leading assignment of most environment variables, so a command
  such as `RUSTUP_TOOLCHAIN=stable cargo test` may prompt (not verified
  for this variable).
- **Pinned toolchain.** `rust-toolchain.toml` makes rustup download the
  pinned toolchain on first use, so an allowed `cargo` command can
  trigger a large download without a `rustup` prompt.
- **Sandboxing is not enabled.** It is the only mechanism that enforces
  filesystem and network limits for shell commands independently of the
  command text. Enabling it and tightening the rules are tracked in
  `docs/roadmap.md`.
