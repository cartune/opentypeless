# M16 — Safer edit learning, capsule drift fix

Date: 2026-10-10. Branch `m16-safe-learning` (on top of m15).

## What went wrong

After one evening of use the learned rules were mostly wording preferences and diff artefacts,
all applied as deterministic global replacements by `correction_rules_exact_apply`:
你→您, 就還→才, 要再→需要, 援→持 (支援→支持 with the shared char stripped), model→Model,
自由→支援, 崔→Threads, 大→魚寶魚媽知名 (a one-character diff artefact). One edit was enough to
create a rule, and nothing checked whether the pair sounded alike.

## What changed

- **Learnability** (`edit_learning/diff.rs`, `is_learnable_pair`): the dictated side needs ≥ 2
  characters; Chinese sides ≤ 6 characters and within 2 of each other; case- or spacing-only
  changes are rejected; a dictated side made only of function characters (的了是在就才要再還你
  您…) is rejected; identical characters in a different order are rejected. Chinese ↔ Chinese
  pairs must **sound alike** (below). Chinese → Latin needs a term-like target (uppercase or
  digit: Sunny, TITA, GrokBot X API). Latin ↔ Latin needs a term-like target or ≥ 50 % spelling
  overlap. A one-character Chinese slip is widened with its shared neighbour (池典→辭典, not 池→辭).
- **Phonetics** (`edit_learning/phonetic.rs`, crate `pinyin` 0.10 with heteronyms): toneless
  readings with Taiwan confusions merged (zh/z, ch/c, sh/s, n/l, in/ing, en/eng, an/ang, ü/u).
  Syllable counts may differ by one; ≥ 60 % of aligned syllables must match (so both syllables of a
  two-syllable word); the same syllables in another order is a rephrase, not a slip.
- **Two sightings** (`edit_learning/mod.rs`, `storage/mod.rs`): the first time a pair is seen it
  is stored as `source = 'pending'`, `enabled = 0` — nothing is applied and no dictionary word is
  added. The same fix a second time promotes it to `learned` + enabled and adds the word. Editing
  back deletes a pending pair or disables a learned rule. Turning a pending rule on by hand in the
  dictionary page confirms it (`set_correction_enabled` promotes the source).
- **UI**: pending rules show a grey 待確認 badge with the toggle off. Pill / toast copy:
  「已記下 A → B，再修一次就套用」 on the first sighting, 「已學會 A → B」 on the second; the
  main window only jumps to the dictionary page for a confirmed rule.
- **Data cleanup on this machine** (sqlite, 2026-10-10): learned rules that fail the new filter
  were deleted (你→您, 就還→才, 要再→需要, 援→持, 自由→支援, 崔→Threads, model→Model,
  哎呦→車友); the rest were converted to pending (T 塔→TITA, 池點→辭典, RockfatherX→GrokBot X API,
  尚寧→Sunny, 也是 JOLI→Joe, 腿好→推好人); learned vocabulary words were removed (您 才 需要 持
  支援 Model 辭典 車友 推好人); proper nouns stayed (Sunny, TITA, Threads, GrokBot X API, Joe).

## Capsule drift

The pill collapsed to a 6 × 6 pt window between runs. AppKit will not make a borderless window
smaller than about 10 pt a side: it kept the origin and padded the frame to 10 × 10, so every
collapse moved the centre 2 pt right and 2 pt down, and the next expansion grew around the shifted
centre. After enough runs the pill was below the bottom edge of the external display
(`CGWindowListCopyWindowInfo` showed it at y = 1085 on a 1080-high screen).

- `commands/capsule.rs`: `anchored_frame` clamps both sides to `MIN_FRAME_SIDE` (10 pt) so the
  centre is preserved exactly; `recover_offscreen_frame` moves a frame whose centre is on no
  screen to the bottom centre of the nearest screen's visible area (80 pt margin, mirroring
  `CAPSULE_BOTTOM_MARGIN`). `CAPSULE_COLLAPSED_SIZE` is 10 × 10.

## Human checks

1. Dictate a sentence with a misrecognised name, fix it → pill says 已記下…; the dictionary page
   shows the pair with 待確認 and the toggle off; the next dictation is unchanged.
2. Fix the same word again → pill says 已學會…; rule on, badge 自動學習; later dictations apply it.
3. Change 你 to 您 in dictated text → nothing is learned (log: `0 learnable edit(s)`).
4. Use the capsule twenty times on the external display → it stays where it was.
