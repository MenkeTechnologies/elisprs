#!/usr/bin/env bash
# fuzz_parity.sh — differential fuzzer: elisprs vs real GNU Emacs.
#
# Generates a seeded corpus of random elisp forms (scripts/fuzz/gen.el), evaluates
# every form under BOTH `emacs -Q --batch -l' (ground truth) and `elisp' (subject)
# through the same driver (scripts/fuzz/drive.el), and reports every form whose
# value — or whose signalled error — differs. Those are parity gaps; the fixed
# ones are recorded in BUGS.md.
#
#   bash scripts/fuzz_parity.sh                     # 500 forms, seed 1
#   bash scripts/fuzz_parity.sh -n 5000 -s 42       # bigger corpus, new seed
#   bash scripts/fuzz_parity.sh -c target/fuzz/corpus.el   # re-check a corpus
#
#   -n N       corpus size (default 500)
#   -s SEED    PRNG seed (default 1); same seed => same corpus, so a divergence
#              reproduces exactly on any machine
#   -d DEPTH   max form nesting (default 3)
#   -c FILE    use an existing corpus instead of generating one
#   -t SECS    batch timeout (default scales with the corpus: 20 + N/50 seconds;
#              a single re-run form always gets 5). It has to scale — a fixed 20s
#              was under what a debug `elisp' needs for a few thousand forms, and
#              a timeout does not show up as a divergence: both engines print
#              <HANG>, which compares equal, so the run reports perfect parity.
#   -S N       delta-debug the first N diverging forms down to a minimal still-
#              diverging form (default 10; 0 disables). A raw hit is a depth-3
#              tree with three distractions bolted onto the one call that
#              actually diverges, so this is usually the difference between
#              "diagnose in a minute" and "diagnose in an hour".
#   -q         summary only
#
# Artifacts land in target/fuzz/ (gitignored): corpus.el, emacs.out, elisp.out,
# diverge.txt (form + both results), shrunk.txt (minimal reproducers), and a
# head-symbol histogram on stdout.
# Exit status is the number of diverging forms (0 = parity), capped at 250.
set -uo pipefail
cd "$(dirname "$0")/.."

N=500 SEED=1 DEPTH=3 CORPUS= TMO= QUIET=0 SHRINK=10
EMACS="${EMACS:-emacs}"
ELISP="${ELISP:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    -n) N="$2"; shift 2 ;;
    -s) SEED="$2"; shift 2 ;;
    -d) DEPTH="$2"; shift 2 ;;
    -c) CORPUS="$2"; shift 2 ;;
    -t) TMO="$2"; shift 2 ;;
    -S) SHRINK="$2"; shift 2 ;;
    -q) QUIET=1; shift ;;
    # Only the header block, not every `#' line in the file: the body's
    # explanatory comments are not usage text.
    -h|--help) perl -ne 'next if $. == 1; last unless /^#/; s/^# ?//; print' "$0"; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

if [ -t 1 ]; then C='\033[36m'; G='\033[32m'; R='\033[31m'; D='\033[2m'; N_='\033[0m'; else C= G= R= D= N_=; fi
say() { printf "${C}==>${N_} %s\n" "$1"; }

abspath() { perl -MCwd=abs_path -e 'print abs_path($ARGV[0]) // ""' "$1"; }

# ---- oracle gate -----------------------------------------------------------
# The ground truth is a (BINARY, ARGV) pair, and BOTH halves are ambient.
#
# The binary half: `$EMACS' is read from the environment and, if it is a bare
# name, resolved through `$PATH' — so a shell rc, a direnv, or a stale wrapper
# silently redirects the oracle. Every expectation in BUGS.md, the tests and the
# examples was measured against one specific Emacs, and a different one does not
# fail loudly: it reports a different set of diverging forms, which reads
# exactly like a regression (or, worse, blesses output the pinned version would
# have rejected). So: resolve to an ABSOLUTE, symlink-free path, print what
# actually answered, and refuse to run at all if it is not the pinned version.
# The expected version is single-sourced from BUGS.md's header rather than typed
# here, so it cannot drift from the document the numbers live in;
# `EMACS_VERSION_EXPECT' overrides it for a deliberate cross-version run.
#
# The argv half is the one that used to be invisible, and it decides answers the
# version number never mentions. `emacs -Q --batch -l FILE' evaluates in
# `*scratch*' under `lisp-interaction-mode'; `emacs --script FILE' evaluates in
# a fundamental-mode ` *load*' buffer. `(char-syntax ?.)' is 95 in the first and
# 46 in the second — same binary, same version, different answer. So the flag
# vectors are named ONCE, below, used for every invocation, and printed in the
# run header; and `scripts/fuzz/entry.el' is run through those exact vectors on
# both engines before the corpus, so a mismatch stops the run instead of
# producing a wall of syntax "divergences" that are really a door mismatch.
ORACLE_RAW=$(command -v "$EMACS" 2>/dev/null || true)
[ -n "$ORACLE_RAW" ] || { echo "no \`$EMACS' on PATH — the fuzzer needs real Emacs as ground truth" >&2; exit 2; }
ORACLE_BIN=$(abspath "$ORACLE_RAW")
[ -x "$ORACLE_BIN" ] || { echo "\`$ORACLE_RAW' does not resolve to an executable file" >&2; exit 2; }

ORACLE_VERSION=$("$ORACLE_BIN" --version 2>/dev/null | head -1 | perl -ne 'print $1 if /GNU Emacs ([0-9]+(?:\.[0-9]+)*)/')
EXPECT="${EMACS_VERSION_EXPECT:-$(perl -ne 'print $1 and last if /checked against \*\*GNU Emacs ([0-9]+(?:\.[0-9]+)*)\*\*/' BUGS.md)}"
if [ -z "$EXPECT" ]; then
  echo "cannot determine the pinned oracle version: no \"checked against **GNU Emacs X.Y**\" line in BUGS.md." >&2
  echo "Restore it, or set EMACS_VERSION_EXPECT explicitly." >&2
  exit 2
fi
if [ -z "$ORACLE_VERSION" ]; then
  echo "\`$ORACLE_BIN --version' did not report a GNU Emacs version — refusing to trust it as ground truth." >&2
  exit 2
fi
if [ "$ORACLE_VERSION" != "$EXPECT" ]; then
  echo "oracle is GNU Emacs $ORACLE_VERSION, but every expectation in this tree was measured against $EXPECT." >&2
  echo "  resolved: $ORACLE_BIN   (from EMACS=${EMACS})" >&2
  echo "  A mismatched oracle reports a different divergence set, not an error." >&2
  echo "  Install $EXPECT, or set EMACS_VERSION_EXPECT=$ORACLE_VERSION to accept this deliberately." >&2
  exit 2
fi

if [ -z "$ELISP" ]; then
  if   [ -x target/debug/elisp ];   then ELISP=target/debug/elisp
  elif [ -x target/release/elisp ]; then ELISP=target/release/elisp
  else echo "no elisp binary — run \`cargo build' first" >&2; exit 2; fi
fi
SUBJECT_BIN=$(abspath "$ELISP")
[ -x "$SUBJECT_BIN" ] || { echo "\`$ELISP' does not resolve to an executable file" >&2; exit 2; }
SUBJECT_VERSION=$("$SUBJECT_BIN" --version 2>/dev/null | perl -pe 's/\e\[[0-9;]*m//g' | perl -ne 'print $1 if /([0-9]+\.[0-9]+\.[0-9]+)/' | head -1)

# ---- the entry point, named once -------------------------------------------
# These two vectors ARE the pinned entry points. `elisp FILE' is deliberately
# the bare positional form: that is elisprs's `-l' column (src/main.rs picks
# `EntryPoint::Load' when `--script' is absent), so it is the one that lines up
# with `emacs -Q --batch -l FILE'. Changing either vector changes the oracle,
# which is why they are here and not inlined at three call sites.
ORACLE_FLAGS=(-Q --batch -l)
SUBJECT_FLAGS=()

# Portable timeout: SIGALRM survives exec, so the alarm set here fires in the
# exec'd engine. GNU `timeout' is not on a stock macOS.
run_to() { # run_to SECS CMD...
  perl -e 'alarm shift; exec @ARGV or die' "$@"
}
run_oracle()  { local t="$1" f="$2"; run_to "$t" "$ORACLE_BIN" "${ORACLE_FLAGS[@]}" "$f"; }
# `${a+"${a[@]}"}' rather than `"${a[@]}"': under `set -u' bash 3.2 (the stock
# macOS shell) treats an empty array expansion as an unbound variable.
run_subject() { local t="$1" f="$2"; run_to "$t" "$SUBJECT_BIN" ${SUBJECT_FLAGS+"${SUBJECT_FLAGS[@]}"} "$f"; }

entry_line() { # entry_line BIN FLAGS... -- prints the human-readable argv
  printf '%s' "$*"
}

if [ "$QUIET" = 0 ]; then
  printf "${C}oracle ${N_} GNU Emacs %s\n" "$ORACLE_VERSION"
  printf "        binary       %s" "$ORACLE_BIN"
  [ "$ORACLE_BIN" = "$ORACLE_RAW" ] || printf " ${D}(via %s)${N_}" "$ORACLE_RAW"
  printf "\n        entry point  %s\n" "$(entry_line "$ORACLE_BIN" "${ORACLE_FLAGS[@]}" scripts/fuzz/drive.el)"
  printf "${C}subject${N_} elisprs %s\n" "${SUBJECT_VERSION:-?}"
  printf "        binary       %s ${D}(mtime %s)${N_}\n" "$SUBJECT_BIN" \
    "$(perl -e 'my @s=stat($ARGV[0]); print scalar localtime($s[9])' "$SUBJECT_BIN")"
  printf "        entry point  %s\n" \
    "$(entry_line "$SUBJECT_BIN" ${SUBJECT_FLAGS+"${SUBJECT_FLAGS[@]}"} scripts/fuzz/drive.el)"
fi

# ---- entry-point gate ------------------------------------------------------
# Same probe, same argv, both engines. Line 1 is the gated state (current buffer
# + a spread of `char-syntax' answers); line 2 is context that is printed but
# never gated.
EP_O=$(run_oracle 20 scripts/fuzz/entry.el 2>/dev/null)
EP_S=$(run_subject 20 scripts/fuzz/entry.el 2>/dev/null)
EP_O1=$(printf '%s\n' "$EP_O" | head -1); EP_O2=$(printf '%s\n' "$EP_O" | perl -ne 'print if $. == 2')
EP_S1=$(printf '%s\n' "$EP_S" | head -1); EP_S2=$(printf '%s\n' "$EP_S" | perl -ne 'print if $. == 2')
if [ -z "$EP_O1" ] || [ -z "$EP_S1" ]; then
  echo "entry-point probe produced no output (oracle=${EP_O1:-<none>} subject=${EP_S1:-<none>})" >&2
  echo "  scripts/fuzz/entry.el must run under both engines before a corpus means anything." >&2
  exit 2
fi
if [ "$EP_O1" != "$EP_S1" ]; then
  echo "ENTRY-POINT MISMATCH — the two columns did not enter through the same door." >&2
  echo "  oracle : $EP_O1" >&2
  echo "  subject: $EP_S1" >&2
  echo "  Every syntax-dependent form in the corpus would diverge for this reason" >&2
  echo "  alone, which is not a parity result. Fix the entry point, then re-run." >&2
  exit 2
fi
[ "$QUIET" = 1 ] || printf "${D}entry-point state agrees: %s${N_}\n${D}context: oracle[%s] subject[%s]${N_}\n" \
  "$EP_O1" "$EP_O2" "$EP_S2"

OUT=target/fuzz
mkdir -p "$OUT"

# ── corpus ───────────────────────────────────────────────────────────────────
if [ -n "$CORPUS" ]; then
  cp "$CORPUS" "$OUT/corpus.el"
else
  say "generating $N forms (seed $SEED, depth $DEPTH)"
  FUZZ_SEED="$SEED" FUZZ_N="$N" FUZZ_DEPTH="$DEPTH" \
    "$ORACLE_BIN" -Q --batch -l scripts/fuzz/gen.el >"$OUT/corpus.el" || exit 2
fi
TOTAL=$(grep -c '' "$OUT/corpus.el")
[ "$TOTAL" -gt 0 ] || { echo "empty corpus" >&2; exit 2; }

# The batch timeout has to scale with the corpus. A fixed 20s was under the
# ~30s a debug `elisp' needs for 6000 forms, so the batch was killed and every
# unprinted form went to one-process-per-form isolation — minutes of wall clock
# for a run that takes 30 seconds. Worse, a timeout is not visible as a
# divergence: both engines emit `<HANG>' and `<HANG>' compares equal to
# `<HANG>', so a run that timed out reports perfect parity. `-t' still wins.
: "${TMO:=$(( 20 + TOTAL / 50 ))}"

# ── evaluate under both engines ──────────────────────────────────────────────
# One process for the whole corpus (fast), then any index the batch failed to
# print — because the engine crashed, hung, or died mid-buffer — is re-run alone
# so a single bad form cannot hide the rest of the corpus.
drive_batch() { # drive_batch ENGINE OUTFILE
  case "$1" in
    emacs) FUZZ_CORPUS="$OUT/corpus.el" run_oracle  "$TMO" scripts/fuzz/drive.el ;;
    elisp) FUZZ_CORPUS="$OUT/corpus.el" run_subject "$TMO" scripts/fuzz/drive.el ;;
  esac >"$2" 2>/dev/null
}

drive_one() { # drive_one ENGINE INDEX -> prints the result line (or a marker)
  local eng="$1" i="$2" line rc
  case "$eng" in
    emacs) line=$(FUZZ_CORPUS="$OUT/corpus.el" FUZZ_START="$i" FUZZ_COUNT=1 \
                    run_oracle 5 scripts/fuzz/drive.el 2>/dev/null) ;;
    elisp) line=$(FUZZ_CORPUS="$OUT/corpus.el" FUZZ_START="$i" FUZZ_COUNT=1 \
                    run_subject 5 scripts/fuzz/drive.el 2>/dev/null) ;;
  esac
  rc=$?
  if [ -n "$line" ]; then printf '%s\n' "$line"
  elif [ "$rc" -eq 142 ] || [ "$rc" -eq 14 ]; then printf '%d\t<HANG>\n' "$i"   # SIGALRM
  else printf '%d\t<CRASH rc=%d>\n' "$i" "$rc"; fi
}

for eng in emacs elisp; do
  say "evaluating $TOTAL forms under $eng"
  drive_batch "$eng" "$OUT/$eng.out"
  # Re-run whatever the batch did not print (crash, hang, or output lost in the
  # dying process's stdio buffer).
  missing=$(perl -e '
    my ($n, $f) = @ARGV; my %seen;
    open my $fh, "<", $f or exit 0;
    while (<$fh>) { $seen{$1} = 1 if /^(\d+)\t/ }
    print join("\n", grep { !$seen{$_} } 0 .. $n - 1), "\n";
  ' "$TOTAL" "$OUT/$eng.out" | grep -c '^[0-9]' || true)
  if [ "${missing:-0}" -gt 0 ]; then
    say "  $missing form(s) unaccounted for under $eng — isolating"
    perl -e '
      my ($n, $f) = @ARGV; my %seen;
      open my $fh, "<", $f or exit 0;
      while (<$fh>) { $seen{$1} = 1 if /^(\d+)\t/ }
      print "$_\n" for grep { !$seen{$_} } 0 .. $n - 1;
    ' "$TOTAL" "$OUT/$eng.out" | while read -r i; do
      [ -n "$i" ] || continue
      drive_one "$eng" "$i" >>"$OUT/$eng.out"
    done
  fi
  sort -n -k1,1 -o "$OUT/$eng.out" "$OUT/$eng.out"
done

# ── compare ──────────────────────────────────────────────────────────────────
: >"$OUT/diverge.txt"
perl -e '
  my ($corpus, $ref, $sub, $out) = @ARGV;
  my (@forms, %r, %s);
  open my $c, "<", $corpus or die; @forms = <$c>; chomp @forms;
  # Explicit loop variables, never $_: `while (<$fh>)` assigns to $_ and would
  # clobber the outer loop pair, silently emptying both result maps — which
  # makes every form compare <MISSING> to <MISSING> and the fuzzer report
  # perfect parity forever.
  for my $pair ([$ref, \%r], [$sub, \%s]) {
    open my $fh, "<", $pair->[0] or die;
    while (my $l = <$fh>) { chomp $l; $pair->[1]{$1} = $2 if $l =~ /^(\d+)\t(.*)$/s }
  }
  open my $o, ">", $out or die;
  my ($bad, $unresolved) = (0, 0);
  for my $i (0 .. $#forms) {
    my ($a, $b) = ($r{$i} // "<MISSING>", $s{$i} // "<MISSING>");
    # A form neither engine produced a value for is NOT parity, even though the
    # two markers are string-equal. Counting it as agreement is how a timed-out
    # run reports 0 divergences.
    if ($a eq $b) {
      $unresolved++ if $a =~ /^<(HANG|CRASH|MISSING)/;
      next;
    }
    $bad++;
    print $o "#$i  $forms[$i]\n  emacs: $a\n  elisp: $b\n\n";
  }
  print "$bad $unresolved\n";
' "$OUT/corpus.el" "$OUT/emacs.out" "$OUT/elisp.out" "$OUT/diverge.txt" >"$OUT/count"
read -r BAD UNRESOLVED <"$OUT/count"
if [ "${UNRESOLVED:-0}" -gt 0 ]; then
  printf "${R}warning: %d form(s) produced no value under EITHER engine${N_} ${D}(hang/crash — not counted as parity)${N_}\n" \
    "$UNRESOLVED"
fi

# How much of the corpus actually MEASURED something. A form the reference could
# not evaluate — `void-function' because the corpus named something Emacs does
# not have — makes both engines signal the same error, and two matching failures
# read as agreement. That is how a mode can score zero divergences while testing
# nothing, so the numbers are printed rather than left to be assumed.
VALUED=$(grep -c '	=' "$OUT/emacs.out" || true)
REFVOID=$(grep -cE '	!\((void-function|void-variable|invalid-function)' "$OUT/emacs.out" || true)
printf "${D}reference produced a value for %s/%s forms; %s could not be evaluated by Emacs at all${N_}\n" \
  "$VALUED" "$TOTAL" "$REFVOID"
if [ "$REFVOID" -gt $((TOTAL / 20)) ]; then
  printf "${R}warning: >5%% of the corpus is void under Emacs — those forms measure nothing${N_}\n"
fi

echo
if [ "$BAD" -eq 0 ]; then
  printf "${G}PARITY: %d/%d forms agree with Emacs.${N_}\n" "$((TOTAL - UNRESOLVED))" "$TOTAL"
  exit 0
fi

printf "${R}%d/%d forms diverge from Emacs${N_}  ${D}(%s)${N_}\n" "$BAD" "$TOTAL" "$OUT/diverge.txt"
echo
say "divergences by head symbol"
# The head symbol of the outermost form is a coarse but effective bucket: it is
# what you grep for in src/builtins.rs to find the offending port.
perl -ne 'print "$1\n" if /^#\d+\s+\((\S+)/' "$OUT/diverge.txt" \
  | sort | uniq -c | sort -rn | head -25

# ── shrink ───────────────────────────────────────────────────────────────────
# Delta-debug each hit to a minimal still-diverging form. `scripts/fuzz/shrink.el'
# proposes candidates (it is a pure syntactic generator and knows nothing about
# which head symbols matter, so it cannot shrink "towards" a bug we already
# believe in); the differential oracle is the only accept test. One process pair
# per round rather than per candidate, because a debug `elisp' costs seconds to
# start and a per-candidate loop would dominate the whole run.
# The signature of an oracle result: `=' for a value, `!SYMBOL' for a signal.
# Shrinking accepts a candidate only if the ORACLE still answers with the same
# signature, not merely if the two engines still disagree. Without that the
# delta-debugger wanders: the first real run took
#
#   (equal (split-string 1.5) (let ((x (and 97 97))) ...))
#     emacs !(wrong-type-argument sequencep 1.5) / elisp !(wrong-type-argument stringp 1.5)
#
# down to `(split-string)', a wrong-number-of-arguments divergence that is a
# genuine bug but a different one — so the minimal form no longer explained the
# hit it came from. Keeping the signature fixed makes the shrinker answer "the
# smallest form with THIS divergence", which is the question being asked.
result_sig() { # result_sig RESULT-LINE
  printf '%s' "$1" | perl -ne 'print /^!\((\S+?)[\s)]/ ? "!$1" : /^!/ ? "!" : "="'
}

eval_oracle_one() { # eval_oracle_one FORM -> the oracle's result for it
  printf '%s\n' "$1" >"$OUT/one.el"
  FUZZ_CORPUS="$OUT/one.el" run_oracle 10 scripts/fuzz/drive.el 2>/dev/null | perl -pe 's/^0\t//'
}

shrink_form() { # shrink_form FORM -> prints the minimal form found
  local cur="$1" round=0 pick lines_o lines_s want
  want=$(result_sig "$(eval_oracle_one "$cur")")
  while [ "$round" -lt 12 ]; do
    FUZZ_FORM="$cur" run_oracle 20 scripts/fuzz/shrink.el 2>/dev/null \
      | perl -ne 'print if $. <= 150' >"$OUT/cands.el"
    [ -s "$OUT/cands.el" ] || break
    FUZZ_CORPUS="$OUT/cands.el" run_oracle  20 scripts/fuzz/drive.el >"$OUT/cands.emacs.out" 2>/dev/null
    FUZZ_CORPUS="$OUT/cands.el" run_subject 20 scripts/fuzz/drive.el >"$OUT/cands.elisp.out" 2>/dev/null
    # A candidate that crashes an engine truncates the rest of that engine's
    # batch, and the truncated tail would then read as "subject produced
    # nothing" for forms it never saw. Bail out rather than shrink towards an
    # artifact of our own batching.
    lines_o=$(grep -c '^[0-9]' "$OUT/cands.emacs.out" || true)
    lines_s=$(grep -c '^[0-9]' "$OUT/cands.elisp.out" || true)
    if [ "$lines_o" -lt "$(grep -c '' "$OUT/cands.el")" ] || [ "$lines_s" -lt "$lines_o" ]; then
      break
    fi
    # First candidate (they arrive smallest-first) whose two results differ.
    pick=$(perl -e '
      my ($a, $b, $want) = @ARGV; my (%x, %y);
      for my $p ([$a, \%x], [$b, \%y]) {
        open my $fh, "<", $p->[0] or exit 0;
        while (my $l = <$fh>) { chomp $l; $p->[1]{$1} = $2 if $l =~ /^(\d+)\t(.*)$/s }
      }
      for my $i (sort { $a <=> $b } keys %x) {
        next unless exists $y{$i};
        # Skip candidates that measure nothing: a form Emacs itself cannot
        # evaluate is not a smaller reproducer, it is a different question.
        next if $x{$i} =~ /^!\((void-function|void-variable|invalid-function)/;
        my $sig = $x{$i} =~ /^!\((\S+?)[\s)]/ ? "!$1" : $x{$i} =~ /^!/ ? "!" : "=";
        next if $sig ne $want;
        if ($x{$i} ne $y{$i}) { print $i; last }
      }
    ' "$OUT/cands.emacs.out" "$OUT/cands.elisp.out" "$want")
    [ -n "$pick" ] || break
    cur=$(perl -ne "print and last if \$. == $((pick + 1))" "$OUT/cands.el")
    [ -n "$cur" ] || break
    round=$((round + 1))
  done
  printf '%s\n' "$cur"
}

: >"$OUT/shrunk.txt"
if [ "$SHRINK" -gt 0 ]; then
  echo
  say "shrinking the first $SHRINK divergence(s)"
  perl -ne 'print "$1\n" if /^#\d+\s+(.*)$/' "$OUT/diverge.txt" | head -"$SHRINK" | while read -r form; do
    [ -n "$form" ] || continue
    min=$(shrink_form "$form")
    {
      printf 'from: %s\n  to: %s\n' "$form" "$min"
      printf '  emacs: %s\n' "$(eval_oracle_one "$min")"
      printf '  elisp: %s\n\n' \
        "$(FUZZ_CORPUS="$OUT/one.el" run_subject 10 scripts/fuzz/drive.el 2>/dev/null | perl -pe 's/^0\t//')"
    } >>"$OUT/shrunk.txt"
  done
  if [ "$QUIET" -eq 0 ]; then cat "$OUT/shrunk.txt"; fi
  say "minimal reproducers in $OUT/shrunk.txt"
fi

if [ "$QUIET" -eq 0 ]; then
  echo
  say "first divergences (unshrunk)"
  head -45 "$OUT/diverge.txt"
fi
[ "$BAD" -gt 250 ] && BAD=250
exit "$BAD"
