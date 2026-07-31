#!/usr/bin/env bash
#
# ci/gates.sh — the project's red lines, checked before anything is built.
#
# The list of paths comes from the CALLER and is never derived inside this
# script. That is not a matter of style: it is what makes the gate testable.
# ci/gates-selftest.sh points this script at a temporary directory holding
# deliberate violations and requires a non-zero exit for each. A gate that
# fetches its own file list has nothing to aim at and cannot be proven.
#
# usage: ci/gates.sh [--gates=all|determinism|text] PATH...
#
#   determinism  floating point, system randomness and wall clock
#   text         U+FFFD, the section sign U+00A7, UTF-8 and NUL in *.md
#   all          both groups, the default
#
# The determinism group is meant for the simulation sources. Pointing it at
# ci/ itself is meaningless: the strings it hunts for live here as patterns.
#
# The section sign and U+FFFD are assembled with printf from their code points
# and never written into this file literally, or the gate would flag itself on
# the first run.

set -uo pipefail

usage() {
    sed -n '3,22p' "$0" >&2
}

gates=all
while [ $# -gt 0 ]; do
    case "$1" in
        --gates=*) gates="${1#--gates=}"; shift ;;
        -h|--help) usage; exit 0 ;;
        --)        shift; break ;;
        -*)        printf 'gates: unknown flag: %s\n' "$1" >&2; usage; exit 2 ;;
        *)         break ;;
    esac
done

case "$gates" in
    all|determinism|text) ;;
    *) printf 'gates: unknown group: %s\n' "$gates" >&2; exit 2 ;;
esac

if [ $# -eq 0 ]; then
    printf 'gates: no paths given\n' >&2
    usage
    exit 2
fi

for path in "$@"; do
    if [ ! -e "$path" ]; then
        printf 'gates: path not found: %s\n' "$path" >&2
        printf '::error::the gate cannot run: path %s is missing\n' "$path" >&2
        exit 2
    fi
done

files=()
while IFS= read -r -d '' file; do
    files+=("$file")
done < <(
    for path in "$@"; do
        if [ -d "$path" ]; then
            find "$path" -type f -print0
        else
            printf '%s\0' "$path"
        fi
    done
)

if [ ${#files[@]} -eq 0 ]; then
    printf 'gates: the given paths hold no files\n' >&2
    exit 2
fi

fails=0

# scan <-E|-F> <pattern> <message shown on a hit>
#
# A grep exit code above one is NOT a clean result. It means the check refused
# to run, and it counts as a failure. The first version of this gate pretended
# to work in exactly that way: grep -P aborted in a non-Unicode locale, the
# error was swallowed by `|| true`, and a planted violation passed straight
# through.
scan() {
    local mode="$1" pattern="$2" message="$3"
    local out rc

    # -I skips binary files. Images arrive in web/ later and the bytes of
    # U+FFFD can occur inside a PNG by chance; a gate that fails on a file
    # where there is no text to search is a false alarm. Under LC_ALL=C the
    # binary test keys on NUL bytes, so a markdown file with broken encoding
    # is still scanned.
    out=$(LC_ALL=C grep -nHI "$mode" -e "$pattern" "${files[@]}" 2>&1)
    rc=$?

    if [ "$rc" -gt 1 ]; then
        printf '%s\n' "$out" >&2
        printf '::error::gate broken: grep exited %s, the check did not run\n' "$rc" >&2
        fails=$((fails + 1))
        return
    fi

    if [ "$rc" -eq 0 ]; then
        printf '%s\n' "$out" >&2
        printf '::error::%s\n' "$message" >&2
        fails=$((fails + 1))
    fi
}

if [ "$gates" = all ] || [ "$gates" = determinism ]; then
    # Written as f(32|64) rather than as two alternatives so that the pattern
    # does not contain the strings it hunts for and cannot flag itself.
    scan -E '\bf(32|64)\b' \
        'floating point is banned: fixed-point Q10 in i32 only, see DETERMINISM.md'

    scan -E 'rand::|thread_rng|SystemTime' \
        'system randomness and the wall clock are banned: explicit xoshiro256++ only, see DETERMINISM.md'
fi

if [ "$gates" = all ] || [ "$gates" = text ]; then
    scan -F "$(printf '\357\277\275')" \
        'U+FFFD: text was decoded wrongly and then saved, the original word is gone'

    scan -F "$(printf '\302\247')" \
        'the section sign U+00A7 is banned as non-ASCII: write references as #N'

    # UTF-8 validity is checked for *.md only. In the other tracked file types
    # broken bytes would already break the compiler or a config parser;
    # markdown is the one place where the damage reaches the repository unseen.
    if ! command -v iconv >/dev/null 2>&1; then
        printf '::error::gate broken: iconv is missing, UTF-8 cannot be checked\n' >&2
        fails=$((fails + 1))
    else
        invalid=""
        withnul=""
        # -I made grep skip binary files, and a NUL byte is what marks a file
        # as binary. NUL is valid UTF-8, so iconv does not object either: a
        # markdown file with a single NUL byte would slip past every text
        # gate, hiding U+FFFD behind it. Legitimate markdown never needs NUL,
        # so its mere presence is a failure.
        for file in "${files[@]}"; do
            case "$file" in
                *.md)
                    nuls=$(LC_ALL=C tr -dc '\000' < "$file" | wc -c)
                    nulrc=$?
                    if [ "$nulrc" -ne 0 ]; then
                        printf '::error::gate broken: counting NUL bytes in %s exited %s, the check did not run\n' \
                            "$file" "$nulrc" >&2
                        fails=$((fails + 1))
                    elif [ "$nuls" -ne 0 ]; then
                        withnul="$withnul $file"
                    fi

                    iconv -f utf-8 -t utf-8 "$file" >/dev/null 2>&1 \
                        || invalid="$invalid $file"
                    ;;
            esac
        done
        if [ -n "$invalid" ]; then
            printf '::error::not valid UTF-8:%s\n' "$invalid" >&2
            fails=$((fails + 1))
        fi
        if [ -n "$withnul" ]; then
            printf '::error::NUL byte in markdown, the file would be skipped as binary:%s\n' \
                "$withnul" >&2
            fails=$((fails + 1))
        fi
    fi
fi

if [ "$fails" -gt 0 ]; then
    printf 'gates: checks failed: %d\n' "$fails" >&2
    exit 1
fi

printf 'gates: clean (group %s, files checked: %d)\n' "$gates" "${#files[@]}"
exit 0
