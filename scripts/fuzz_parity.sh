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
#   -t SECS    per-process timeout (default 20 batch, 5 single-form)
#   -q         summary only
#
# Artifacts land in target/fuzz/ (gitignored): corpus.el, emacs.out, elisp.out,
# diverge.txt (form + both results), and a head-symbol histogram on stdout.
# Exit status is the number of diverging forms (0 = parity), capped at 250.
set -uo pipefail
cd "$(dirname "$0")/.."

N=500 SEED=1 DEPTH=3 CORPUS= TMO=20 QUIET=0
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
    -h|--help) grep '^#' "$0" | cut -c3- ; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

if [ -t 1 ]; then C='\033[36m'; G='\033[32m'; R='\033[31m'; D='\033[2m'; N_='\033[0m'; else C= G= R= D= N_=; fi
say() { printf "${C}==>${N_} %s\n" "$1"; }

command -v "$EMACS" >/dev/null 2>&1 || { echo "no \`$EMACS' on PATH — the fuzzer needs real Emacs as ground truth" >&2; exit 2; }
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
  my $bad = 0;
  for my $i (0 .. $#forms) {
    my ($a, $b) = ($r{$i} // "<MISSING>", $s{$i} // "<MISSING>");
    next if $a eq $b;
    $bad++;
    print $o "#$i  $forms[$i]\n  emacs: $a\n  elisp: $b\n\n";
  }
  print "$bad\n";
' "$OUT/corpus.el" "$OUT/emacs.out" "$OUT/elisp.out" "$OUT/diverge.txt" >"$OUT/count"
BAD=$(cat "$OUT/count")

echo
if [ "$BAD" -eq 0 ]; then
  printf "${G}PARITY: %d/%d forms agree with Emacs.${N_}\n" "$TOTAL" "$TOTAL"
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
