#!/usr/bin/env bash
#
# ci/gates-selftest.sh — proof that ci/gates.sh actually works.
#
# A green gate over a clean tree is indistinguishable from a gate that checks
# nothing. So before the real run the script is pointed at files carrying
# deliberate violations and must return a non-zero code for every one of them,
# and zero for a clean file. If that does not hold, the build fails saying so
# in as many words: the gate has not proven itself.
#
# This runs on EVERY build, not once on a throwaway branch, so that no red run
# has to be left behind in the history to demonstrate the point.
#
# There are six violators rather than five: the section sign shares a gate
# with U+FFFD and would otherwise go unproven. The NUL violator carries a
# U+FFFD of its own, so the NUL check is the only one it can fail: take that
# check away and the file passes everything else, which is precisely the
# regression this violator exists to catch.
#
# The offending characters are assembled with printf from their code points and
# never reach the repository: the directory is created outside the working copy
# and removed on exit.

set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/.." && pwd)
gates="$here/gates.sh"

if [ ! -f "$gates" ]; then
    printf '::error::self-test impossible: %s is missing\n' "$gates" >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# The directory must lie outside the repository. Otherwise the planted
# violations would be swept up by the real run of the gate, and by a commit if
# the cleanup ever failed.
case "$tmp/" in
    "$repo"/*)
        printf '::error::self-test aborted: temporary directory %s is inside the repository %s\n' \
            "$tmp" "$repo" >&2
        exit 1
        ;;
esac

fffd=$(printf '\357\277\275')
sign=$(printf '\302\247')

printf 'let x: f64 = 1.0;\n'                       > "$tmp/violator-float.rs"
printf 'let mut r = rand::thread_rng();\n'         > "$tmp/violator-entropy.rs"
printf 'broken text %s inside a line\n' "$fffd"    > "$tmp/violator-fffd.md"
printf 'a reference written as DOC %s1.1\n' "$sign" > "$tmp/violator-section.md"
printf 'a broken byte \xd0 in the middle\n'        > "$tmp/violator-utf8.md"
printf 'hidden %s behind a NUL \x00 byte\n' "$fffd" > "$tmp/violator-nul.md"
printf 'a clean file, see DETERMINISM.md #2\n'     > "$tmp/clean.md"

fails=0

expect_caught() { # expect_caught <file> <what is being proven>
    local file="$1" what="$2"
    if bash "$gates" --gates=all "$file" >/dev/null 2>&1; then
        printf 'FAILED  not caught: %s (%s)\n' "$what" "$(basename "$file")" >&2
        fails=$((fails + 1))
    else
        printf 'ok      caught: %s\n' "$what"
    fi
}

expect_clean() { # expect_clean <file>
    local file="$1"
    if bash "$gates" --gates=all "$file" >/dev/null 2>&1; then
        printf 'ok      clean file passes: %s\n' "$(basename "$file")"
    else
        printf 'FAILED  clean file rejected: %s\n' "$(basename "$file")" >&2
        fails=$((fails + 1))
    fi
}

expect_caught "$tmp/violator-float.rs"   'floating point'
expect_caught "$tmp/violator-entropy.rs" 'system randomness'
expect_caught "$tmp/violator-fffd.md"    'U+FFFD'
expect_caught "$tmp/violator-section.md" 'section sign U+00A7'
expect_caught "$tmp/violator-utf8.md"    'invalid UTF-8'
expect_caught "$tmp/violator-nul.md"     'NUL byte in markdown'
expect_clean  "$tmp/clean.md"

# Separate check: a missing path must be an error too, not "nothing to scan,
# therefore clean".
if bash "$gates" --gates=all "$tmp/no-such-file" >/dev/null 2>&1; then
    printf 'FAILED  a missing path was taken for a clean one\n' >&2
    fails=$((fails + 1))
else
    printf 'ok      caught: missing path\n'
fi

if [ "$fails" -gt 0 ]; then
    printf '\n::error::the gate has not proven itself: %d self-test check(s) failed. A green run of the gate in this state means nothing\n' \
        "$fails" >&2
    exit 1
fi

printf '\nself-test passed: every violator caught, clean file untouched\n'
exit 0
