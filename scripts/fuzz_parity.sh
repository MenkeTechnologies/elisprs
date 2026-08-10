#!/usr/bin/env bash
# fuzz_parity.sh — differential fuzzer: elisprs vs real GNU Emacs.
#
# Generates a seeded corpus of random elisp forms (scripts/fuzz/gen.el), evaluates
# every form under BOTH `emacs -Q --batch' (ground truth) and `elisp' (subject)
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
#   -q         summary only
#
# Artifacts land in target/fuzz/ (gitignored): corpus.el, emacs.out, elisp.out,
# diverge.txt (form + both results), and a head-symbol histogram on stdout.
# Exit status is the number of diverging forms (0 = parity), capped at 250.
set -uo pipefail
cd "$(dirname "$0")/.."

N=500 SEED=1 DEPTH=3 CORPUS= TMO= QUIET=0
EMACS="${EMACS:-emacs}"
ELISP="${ELISP:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    -n) N="$2"; shift 2 ;;
    -s) SEED="$2"; shift 2 ;;
    -d) DEPTH="$2"; shift 2 ;;
    -c) CORPUS="$2"; shift 2 ;;
    -t) TMO="$2"; shift 2 ;;
    -q) QUIET=1; shift ;;
    # Only the header block, not every `#' line in the file: the body's
    # explanatory comments are not usage text.
    -h|--help) perl -ne 'next if $. == 1; last unless /^#/; s/^# ?//; print' "$0"; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

if [ -t 1 ]; then C='\033[36m'; G='\033[32m'; R='\033[31m'; D='\033[2m'; N_='\033[0m'; else C= G= R= D= N_=; fi
say() { printf "${C}==>${N_} %s\n" "$1"; }

command -v "$EMACS" >/dev/null 2>&1 || { echo "no \`$EMACS' on PATH — the fuzzer needs real Emacs as ground truth" >&2; exit 2; }

# ---- oracle gate -----------------------------------------------------------
# The ground truth is whatever `$EMACS' resolves to, and `$EMACS' is read from
# the environment — so an ambient setting silently redirects the oracle. Every
# expectation in BUGS.md, the tests and the examples was measured against one
# specific Emacs, and a different one does not fail loudly: it reports a
# different set of diverging forms, which reads exactly like a regression (or,
# worse, blesses output that the pinned version would have rejected).
#
# So: resolve the oracle, print what actually answered, and refuse to run at all
# if it is not the pinned version. The expected version is single-sourced from
# BUGS.md's header rather than typed here, so it cannot drift from the document
# the numbers live in; `EMACS_VERSION_EXPECT' overrides it for a deliberate
# cross-version run.
ORACLE_PATH=$(command -v "$EMACS")
ORACLE_VERSION=$("$EMACS" --version 2>/dev/null | head -1 | perl -ne 'print $1 if /GNU Emacs ([0-9]+(?:\.[0-9]+)*)/')
EXPECT="${EMACS_VERSION_EXPECT:-$(perl -ne 'print $1 and last if /checked against \*\*GNU Emacs ([0-9]+(?:\.[0-9]+)*)\*\*/' BUGS.md)}"
if [ -z "$EXPECT" ]; then
  echo "cannot determine the pinned oracle version: no \"checked against **GNU Emacs X.Y**\" line in BUGS.md." >&2
  echo "Restore it, or set EMACS_VERSION_EXPECT explicitly." >&2
  exit 2
fi
if [ -z "$ORACLE_VERSION" ]; then
  echo "\`$ORACLE_PATH --version' did not report a GNU Emacs version — refusing to trust it as ground truth." >&2
  exit 2
fi
if [ "$ORACLE_VERSION" != "$EXPECT" ]; then
  echo "oracle is GNU Emacs $ORACLE_VERSION, but every expectation in this tree was measured against $EXPECT." >&2
  echo "  resolved: $ORACLE_PATH   (from EMACS=${EMACS})" >&2
  echo "  A mismatched oracle reports a different divergence set, not an error." >&2
  echo "  Install $EXPECT, or set EMACS_VERSION_EXPECT=$ORACLE_VERSION to accept this deliberately." >&2
  exit 2
fi
[ "$QUIET" = 1 ] || printf "oracle: GNU Emacs %s at %s\n" "$ORACLE_VERSION" "$ORACLE_PATH"
if [ -z "$ELISP" ]; then
  if   [ -x target/debug/elisp ];   then ELISP=target/debug/elisp
  elif [ -x target/release/elisp ]; then ELISP=target/release/elisp
  else echo "no elisp binary — run \`cargo build' first" >&2; exit 2; fi
fi

OUT=target/fuzz
mkdir -p "$OUT"

# Portable timeout: SIGALRM survives exec, so the alarm set here fires in the
# exec'd engine. GNU `timeout' is not on a stock macOS.
run_to() { # run_to SECS CMD...
  perl -e 'alarm shift; exec @ARGV or die' "$@"
}

# ── corpus ───────────────────────────────────────────────────────────────────
if [ -n "$CORPUS" ]; then
  cp "$CORPUS" "$OUT/corpus.el"
else
  say "generating $N forms (seed $SEED, depth $DEPTH)"
  FUZZ_SEED="$SEED" FUZZ_N="$N" FUZZ_DEPTH="$DEPTH" \
    "$EMACS" -Q --batch -l scripts/fuzz/gen.el >"$OUT/corpus.el" || exit 2
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
    emacs) FUZZ_CORPUS="$OUT/corpus.el" run_to "$TMO" "$EMACS" -Q --batch -l scripts/fuzz/drive.el ;;
    elisp) FUZZ_CORPUS="$OUT/corpus.el" run_to "$TMO" "$ELISP" scripts/fuzz/drive.el ;;
  esac >"$2" 2>/dev/null
}

drive_one() { # drive_one ENGINE INDEX -> prints the result line (or a marker)
  local eng="$1" i="$2" line rc
  case "$eng" in
    emacs) line=$(FUZZ_CORPUS="$OUT/corpus.el" FUZZ_START="$i" FUZZ_COUNT=1 \
                    run_to 5 "$EMACS" -Q --batch -l scripts/fuzz/drive.el 2>/dev/null) ;;
    elisp) line=$(FUZZ_CORPUS="$OUT/corpus.el" FUZZ_START="$i" FUZZ_COUNT=1 \
                    run_to 5 "$ELISP" scripts/fuzz/drive.el 2>/dev/null) ;;
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
if [ "$QUIET" -eq 0 ]; then
  echo
  say "first divergences"
  head -45 "$OUT/diverge.txt"
fi
[ "$BAD" -gt 250 ] && BAD=250
exit "$BAD"
