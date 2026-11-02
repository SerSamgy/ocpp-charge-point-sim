#!/usr/bin/env bash
# PreToolUse hook of the pr-reviewer subagent (.claude/agents/pr-reviewer.md).
# Lets a Bash command through only if every part of it reads the
# repository, builds it or runs the checks. Anything else is blocked with
# exit code 2; the reason on stderr goes back to the subagent.
#
# An allowlist that fails closed: a command it cannot parse is blocked,
# e.g. `grep 'a|b'`, whose quoted `|` is taken for a pipe. It matches
# command text like the permission rules (ADR 0005), so it stops mistakes,
# not a determined attacker: allowed commands such as `cargo test` still
# run repository code.
set -euo pipefail
# Any other exit code would let the command run, so fail closed.
trap 'echo "pr-reviewer-bash: internal error, command blocked" >&2; exit 2' ERR

cmd=$(jq -r '.tool_input.command // empty')
[ -z "$cmd" ] && exit 0

deny() {
    echo "pr-reviewer-bash: blocked ($1). The PR reviewer may only read," \
        "build and run checks; report what you could not check instead." >&2
    exit 2
}

# Redirections that write nothing, removed before the checks below.
rest=$(sed -E 's/[0-9]?>&[0-9]//g; s#[0-9&]?>+ ?/dev/null##g' <<<"$cmd")
case $rest in
    *'>'*) deny "output redirection" ;;
    *'$('* | *'`'* | *'<('*) deny "command substitution" ;;
esac

# One segment per simple command; a lone `&` left over means a
# background job.
segments=$(sed -E 's/&&|\|\||;|\|/\n/g' <<<"$rest")
case $segments in
    *'&'*) deny "background job" ;;
esac

has_arg() { # has_arg <regex> <words...>: some word matches the regex
    local re=$1 w
    shift
    for w in "$@"; do
        [[ $w =~ $re ]] && return 0
    done
    return 1
}

while IFS= read -r segment; do
    read -ra w <<<"$segment"
    [ "${#w[@]}" -eq 0 ] && continue
    case ${w[0]} in
        git)
            case ${w[1]:-} in
                log | show | diff | status | rev-parse | rev-list | merge-base | \
                    ls-files | cat-file | blame | shortlog | describe | archive) ;;
                branch) [[ ${w[2]:-} =~ ^--(show-current|list)$ ]] || deny "git branch ${w[2]:-}" ;;
                *) deny "git ${w[1]:-}" ;;
            esac
            ! has_arg '^(--output|-o)' "${w[@]:2}" || deny "git writing to a file"
            ;;
        cargo)
            case ${w[1]:-} in
                test | build | check | clippy | tree | metadata) ;;
                fmt) has_arg '^--check$' "${w[@]:2}" || deny "cargo fmt without --check" ;;
                *) deny "cargo ${w[1]:-}" ;;
            esac
            ! has_arg '^--fix' "${w[@]:2}" || deny "cargo --fix"
            ;;
        just)
            case ${w[1]:-} in
                '' | --list | -l | ci | ci-rust | ci-python | test | check-docs) [ "${#w[@]}" -le 2 ] || deny "arguments to just ${w[1]}" ;;
                test-commits) [ "${#w[@]}" -le 3 ] && [[ ${w[2]:-main} =~ ^[A-Za-z0-9._/~^-]+$ ]] || deny "just test-commits ${w[*]:2}" ;;
                *) deny "just ${w[1]}" ;;
            esac
            ;;
        gh)
            case "${w[1]:-} ${w[2]:-}" in
                'pr view' | 'pr diff' | 'pr checks' | 'pr list' | 'run view' | 'run list') ;;
                *) deny "gh ${w[1]:-} ${w[2]:-}" ;;
            esac
            ;;
        sort) ! has_arg '^(-o|--output)' "${w[@]:1}" || deny "sort writing to a file" ;;
        ls | cat | head | tail | wc | grep | uniq | cut | tr | diff | pwd | echo | cd) ;;
        *) deny "${w[0]} is not on the list" ;;
    esac
done <<<"$segments"

exit 0
