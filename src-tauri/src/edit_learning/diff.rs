//! Pure logic for learning from the user's edits.
//!
//! After dictated text lands in the target field we remember where it sits
//! (`SpanAnchor`); later reads of the field are re-located through the anchor
//! and diffed against the baseline. Only small, word-sized replacements are
//! reported as `(from, to)` pairs. Everything else (appended typing, deleted
//! sentences, full rewrites) is ignored so the dictionary never learns noise.

/// A word-sized replacement the user made inside the dictated span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedEdit {
    pub from: String,
    pub to: String,
}

/// Where the dictated text sits in the field right after insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanAnchor {
    /// Up to `CONTEXT_CHARS` characters before the span (empty at field start).
    pub before: Vec<char>,
    /// The span as the field holds it (after the app's own autocorrect).
    pub span: Vec<char>,
    /// Up to `CONTEXT_CHARS` characters after the span (empty at field end).
    pub after: Vec<char>,
}

/// Characters of surrounding text kept on each side of the span.
pub const CONTEXT_CHARS: usize = 24;
/// Fields longer than this are left alone (documents, logs, editors).
pub const MAX_FIELD_CHARS: usize = 40_000;
/// Dictations longer than this are left alone.
pub const MAX_SPAN_CHARS: usize = 3_000;
/// More replacements than this means the user rewrote the text.
const MAX_HUNKS: usize = 5;
/// Changing more than this share of the span means a rewrite, not a fix.
const MAX_CHANGED_RATIO: f32 = 0.4;
/// Below this span length the ratio rule is meaningless (one term is most of it).
const MIN_SPAN_FOR_RATIO: usize = 20;
/// Longest side of a learned pair.
const MAX_SIDE_CHARS: usize = 24;
/// Longest Latin phrase (in words) of a learned pair.
const MAX_LATIN_WORDS: usize = 3;
/// Longest CJK side of a learned pair; terms are short, phrases are edits.
const MAX_CJK_CHARS: usize = 6;
/// A one-character dictated side (大, 你, 援) matches everywhere; never learn it.
const MIN_FROM_CHARS: usize = 2;
/// Latin → Chinese fixes stay short.
const MAX_LATIN_TO_CJK_CHARS: usize = 4;
/// Largest diff table we are willing to fill (cells).
const MAX_DP_CELLS: usize = 4_000_000;
/// How similar the caret-anchored slice must be to the inserted text.
const MIN_CARET_SIMILARITY: f32 = 0.8;
const MAX_OCCURRENCES: usize = 64;

/// Build the anchor from the first read after insertion. `caret` is the
/// caret position in characters (the end of the inserted text); when it is
/// missing or does not line up, the inserted text is searched for instead.
pub fn anchor_from_caret(
    value: &[char],
    caret: Option<usize>,
    inserted: &str,
) -> Option<SpanAnchor> {
    let inserted: Vec<char> = inserted.chars().collect();
    if inserted.is_empty() || inserted.len() > MAX_SPAN_CHARS || value.len() > MAX_FIELD_CHARS {
        return None;
    }
    let n = inserted.len();
    let caret_range = caret
        .filter(|&c| c >= n && c <= value.len())
        .map(|c| (c - n, c))
        .filter(|&(s, e)| similarity(&value[s..e], &inserted) >= MIN_CARET_SIMILARITY);
    let (start, end) = match caret_range {
        Some(range) => range,
        None => {
            let hits = find_all(value, &inserted);
            match hits.as_slice() {
                [only] => (*only, *only + n),
                _ => return None,
            }
        }
    };
    Some(SpanAnchor {
        before: value[start.saturating_sub(CONTEXT_CHARS)..start].to_vec(),
        span: value[start..end].to_vec(),
        after: value[end..(end + CONTEXT_CHARS).min(value.len())].to_vec(),
    })
}

/// Find the span in a later read of the field using the surrounding context.
/// Returns the character range, or `None` when the context is gone.
pub fn locate_span(anchor: &SpanAnchor, value: &[char]) -> Option<(usize, usize)> {
    if value.len() > MAX_FIELD_CHARS {
        return None;
    }
    let starts: Vec<usize> = if anchor.before.is_empty() {
        vec![0]
    } else {
        find_all(value, &anchor.before)
            .into_iter()
            .map(|at| at + anchor.before.len())
            .collect()
    };
    let ends: Vec<usize> = if anchor.after.is_empty() {
        vec![value.len()]
    } else {
        find_all(value, &anchor.after)
    };
    let target = anchor.span.len() as i64;
    let mut best: Option<(usize, usize, i64)> = None;
    for &s in &starts {
        for &e in &ends {
            if e < s {
                continue;
            }
            let score = ((e - s) as i64 - target).abs();
            if best.is_none_or(|(_, _, b)| score < b) {
                best = Some((s, e, score));
            }
        }
    }
    best.map(|(s, e, _)| (s, e))
}

/// Compare the span as first seen with how it reads now and return the
/// word-sized replacements, or nothing when the change is not a fix.
pub fn learn_edits(baseline: &[char], current: &[char]) -> Vec<LearnedEdit> {
    if baseline == current || baseline.is_empty() {
        return Vec::new();
    }
    let prefix = baseline
        .iter()
        .zip(current)
        .take_while(|(a, b)| a == b)
        .count();
    let max_suffix = baseline.len().min(current.len()) - prefix;
    let suffix = baseline[prefix..]
        .iter()
        .rev()
        .zip(current[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(max_suffix);
    let mid_b = &baseline[prefix..baseline.len() - suffix];
    let mid_c = &current[prefix..current.len() - suffix];
    if mid_b.len().saturating_mul(mid_c.len()) > MAX_DP_CELLS {
        return Vec::new();
    }
    let mut hunks = hunks_from_alignment(&align(mid_b, mid_c), prefix);
    merge_cjk_neighbours(&mut hunks, baseline);
    expand_latin_words(&mut hunks, baseline, current);
    merge_touching(&mut hunks);

    // Pure insertions (typing on) and pure deletions are not fixes; only
    // replacements count, and only their dictated side is measured.
    let replacements: Vec<Hunk> = hunks
        .into_iter()
        .filter(|h| h.b_end > h.b_start && h.c_end > h.c_start)
        .collect();
    let changed: usize = replacements.iter().map(|h| h.b_end - h.b_start).sum();
    // One swapped term in a short dictation is most of it, so the ratio only
    // speaks for long spans or when several terms changed at once.
    let rewrite = (baseline.len() >= MIN_SPAN_FOR_RATIO || replacements.len() >= 2)
        && changed as f32 > baseline.len() as f32 * MAX_CHANGED_RATIO;
    if replacements.len() > MAX_HUNKS || rewrite {
        return Vec::new();
    }
    let mut edits: Vec<LearnedEdit> = Vec::new();
    for hunk in replacements {
        let hunk = widen_single_cjk_char(hunk, baseline, current);
        let from = trim_edges(&baseline[hunk.b_start..hunk.b_end]);
        let to = trim_edges(&current[hunk.c_start..hunk.c_end]);
        if !is_learnable_pair(&from, &to) {
            continue;
        }
        if !edits.iter().any(|e| e.from == from && e.to == to) {
            edits.push(LearnedEdit { from, to });
        }
    }
    edits
}

/// Convert a UTF-16 offset (what Accessibility reports) to a char index.
pub fn utf16_offset_to_char_index(value: &[char], offset: usize) -> Option<usize> {
    let mut units = 0usize;
    for (index, ch) in value.iter().enumerate() {
        if units == offset {
            return Some(index);
        }
        if units > offset {
            return None;
        }
        units += ch.len_utf16();
    }
    (units == offset).then_some(value.len())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hunk {
    b_start: usize,
    b_end: usize,
    c_start: usize,
    c_end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Del,
    Ins,
}

/// Character-level LCS alignment of two short slices.
fn align(a: &[char], b: &[char]) -> Vec<Op> {
    let (n, m) = (a.len(), b.len());
    let width = m + 1;
    let mut table = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i * width + j] = if a[i] == b[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let mut ops = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push(Op::Eq);
            i += 1;
            j += 1;
        } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
            ops.push(Op::Del);
            i += 1;
        } else {
            ops.push(Op::Ins);
            j += 1;
        }
    }
    ops.extend(std::iter::repeat_n(Op::Del, n - i));
    ops.extend(std::iter::repeat_n(Op::Ins, m - j));
    ops
}

fn hunks_from_alignment(ops: &[Op], offset: usize) -> Vec<Hunk> {
    let mut hunks = Vec::new();
    let (mut b, mut c) = (offset, offset);
    let mut open: Option<Hunk> = None;
    for op in ops {
        match op {
            Op::Eq => {
                if let Some(h) = open.take() {
                    hunks.push(h);
                }
                b += 1;
                c += 1;
            }
            Op::Del | Op::Ins => {
                let h = open.get_or_insert(Hunk {
                    b_start: b,
                    b_end: b,
                    c_start: c,
                    c_end: c,
                });
                if *op == Op::Del {
                    b += 1;
                    h.b_end = b;
                } else {
                    c += 1;
                    h.c_end = c;
                }
            }
        }
    }
    if let Some(h) = open {
        hunks.push(h);
    }
    hunks
}

/// 開會→會議 aligns as 開→∅ + 會 + ∅→議; join hunks split by one CJK char.
fn merge_cjk_neighbours(hunks: &mut Vec<Hunk>, baseline: &[char]) {
    let mut merged: Vec<Hunk> = Vec::with_capacity(hunks.len());
    for hunk in hunks.drain(..) {
        if let Some(last) = merged.last_mut() {
            let gap = hunk.b_start - last.b_end;
            if gap <= 1
                && baseline[last.b_end..hunk.b_start]
                    .iter()
                    .all(|c| is_cjk(*c))
            {
                last.b_end = hunk.b_end;
                last.c_end = hunk.c_end;
                continue;
            }
        }
        merged.push(hunk);
    }
    *hunks = merged;
}

/// cartoon→Cartune aligns as "oo"→"u" inside a word; widen to whole words.
fn expand_latin_words(hunks: &mut [Hunk], baseline: &[char], current: &[char]) {
    for h in hunks.iter_mut() {
        let touches_word = |b: usize, c: usize| {
            baseline.get(b).is_some_and(|ch| is_word_char(*ch))
                || current.get(c).is_some_and(|ch| is_word_char(*ch))
        };
        while h.b_start > 0
            && h.c_start > 0
            && is_word_char(baseline[h.b_start - 1])
            && baseline[h.b_start - 1] == current[h.c_start - 1]
            && (h.b_start == h.b_end && h.c_start == h.c_end || touches_word(h.b_start, h.c_start))
        {
            h.b_start -= 1;
            h.c_start -= 1;
        }
        while h.b_end < baseline.len()
            && h.c_end < current.len()
            && is_word_char(baseline[h.b_end])
            && baseline[h.b_end] == current[h.c_end]
            && (h.b_end > h.b_start && is_word_char(baseline[h.b_end - 1])
                || h.c_end > h.c_start && is_word_char(current[h.c_end - 1]))
        {
            h.b_end += 1;
            h.c_end += 1;
        }
    }
}

fn merge_touching(hunks: &mut Vec<Hunk>) {
    let mut merged: Vec<Hunk> = Vec::with_capacity(hunks.len());
    for hunk in hunks.drain(..) {
        if let Some(last) = merged.last_mut() {
            if hunk.b_start <= last.b_end {
                last.b_end = last.b_end.max(hunk.b_end);
                last.c_end = last.c_end.max(hunk.c_end);
                continue;
            }
        }
        merged.push(hunk);
    }
    *hunks = merged;
}

/// A one-character Chinese slip (池典 → 辭典) must not become a rule for that
/// character alone; take the shared neighbour (left first, else right) so
/// the rule stays scoped to the word it was fixed in.
fn widen_single_cjk_char(hunk: Hunk, baseline: &[char], current: &[char]) -> Hunk {
    let single = hunk.b_end - hunk.b_start == 1
        && hunk.c_end - hunk.c_start == 1
        && is_cjk(baseline[hunk.b_start])
        && is_cjk(current[hunk.c_start]);
    if !single {
        return hunk;
    }
    let left_shared = hunk.b_start > 0
        && hunk.c_start > 0
        && baseline[hunk.b_start - 1] == current[hunk.c_start - 1]
        && is_cjk(baseline[hunk.b_start - 1]);
    if left_shared {
        return Hunk {
            b_start: hunk.b_start - 1,
            c_start: hunk.c_start - 1,
            ..hunk
        };
    }
    let right_shared = hunk.b_end < baseline.len()
        && hunk.c_end < current.len()
        && baseline[hunk.b_end] == current[hunk.c_end]
        && is_cjk(baseline[hunk.b_end]);
    if right_shared {
        return Hunk {
            b_end: hunk.b_end + 1,
            c_end: hunk.c_end + 1,
            ..hunk
        };
    }
    hunk
}

fn trim_edges(chars: &[char]) -> String {
    chars
        .iter()
        .collect::<String>()
        .trim_matches(|c: char| c.is_whitespace() || is_sentence_punct(c))
        .to_string()
}

/// Characters that carry grammar rather than meaning. A dictated side made
/// only of these is a wording preference (你→您, 就還→才), never a slip.
const FUNCTION_CHARS: &str =
    "的了是在就才要再還你您我他她它們這那有不也都很會可以和跟與把被對沒好嗎呢吧啊喔哦欸嘿呀吶嘛過著";
/// Longest difference in character count between two Chinese sides.
const MAX_CJK_LENGTH_DIFF: usize = 2;
/// Latin-to-Latin fixes must keep at least this much of the spelling unless
/// the corrected side looks like a product or person name.
const MIN_LATIN_SIMILARITY: f32 = 0.5;

/// Would learning `from → to` as a global replacement be safe? Only edits
/// that look like a recognition slip qualify: short, not a single character,
/// not a change of wording, and (for Chinese) sounding like what was said.
pub fn is_learnable_pair(from: &str, to: &str) -> bool {
    if from.is_empty() || to.is_empty() || from == to {
        return false;
    }
    let shape_ok = [from, to].iter().all(|side| {
        let count = side.chars().count();
        let cjk = side.chars().any(is_cjk);
        count <= MAX_SIDE_CHARS
            && side.chars().any(|c| c.is_alphanumeric())
            && !side.chars().any(|c| c == '\n' || is_sentence_punct(c))
            && if cjk {
                count <= MAX_CJK_CHARS
            } else {
                side.split_whitespace().count() <= MAX_LATIN_WORDS
            }
    });
    if !shape_ok || from.chars().count() < MIN_FROM_CHARS {
        return false;
    }
    // Case or spacing only (model → Model) would restyle every occurrence.
    if loose(from) == loose(to) {
        return false;
    }
    let from_cjk = from.chars().any(is_cjk);
    let to_cjk = to.chars().any(is_cjk);
    if from_cjk
        && from
            .chars()
            .all(|c| FUNCTION_CHARS.contains(c) || !is_cjk(c))
    {
        return false;
    }
    match (from_cjk, to_cjk) {
        (true, true) => {
            let from_len = from.chars().count();
            let to_len = to.chars().count();
            if from_len.abs_diff(to_len) > MAX_CJK_LENGTH_DIFF {
                return false;
            }
            // The same characters in another order is the user rephrasing.
            if sorted_chars(from) == sorted_chars(to) {
                return false;
            }
            match super::phonetic::sounds_alike(from, to) {
                Some(alike) => alike,
                None => char_overlap(from, to) >= MIN_LATIN_SIMILARITY,
            }
        }
        // 尚寧 → Sunny, T 塔 → TITA: a name the recogniser could not spell.
        (true, false) => is_term_like(to),
        // Yuma → 魚媽: the reverse slip, only for short names.
        (false, true) => is_term_like(from) && to.chars().count() <= MAX_LATIN_TO_CJK_CHARS,
        (false, false) => {
            is_term_like(to)
                || similarity(
                    &loose(from).chars().collect::<Vec<_>>(),
                    &loose(to).chars().collect::<Vec<_>>(),
                ) >= MIN_LATIN_SIMILARITY
        }
    }
}

fn loose(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn sorted_chars(text: &str) -> Vec<char> {
    let mut chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    chars.sort_unstable();
    chars
}

fn char_overlap(a: &str, b: &str) -> f32 {
    let a_chars: Vec<char> = a.chars().collect();
    let shared = b.chars().filter(|c| a_chars.contains(c)).count();
    shared as f32 / a_chars.len().max(b.chars().count()).max(1) as f32
}

/// Brand and person names: an uppercase letter or a digit somewhere.
fn is_term_like(text: &str) -> bool {
    text.chars().any(|c| c.is_uppercase() || c.is_ascii_digit())
}

fn similarity(a: &[char], b: &[char]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.len().saturating_mul(b.len()) > MAX_DP_CELLS {
        return 0.0;
    }
    let matched = align(a, b).iter().filter(|op| **op == Op::Eq).count();
    matched as f32 / a.len().max(b.len()) as f32
}

fn find_all(haystack: &[char], needle: &[char]) -> Vec<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return Vec::new();
    }
    haystack
        .windows(needle.len())
        .enumerate()
        .filter(|(_, window)| *window == needle)
        .map(|(at, _)| at)
        .take(MAX_OCCURRENCES)
        .collect()
}

pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x3040..=0x30FF | 0xAC00..=0xD7AF | 0x20000..=0x2FA1F)
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '\'' || c == '-' || c == '_'
}

fn is_sentence_punct(c: char) -> bool {
    matches!(
        c,
        '.' | ','
            | ';'
            | ':'
            | '!'
            | '?'
            | '。'
            | '，'
            | '；'
            | '：'
            | '！'
            | '？'
            | '、'
            | '"'
            | '“'
            | '”'
            | '('
            | ')'
            | '（'
            | '）'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn pairs(edits: &[LearnedEdit]) -> Vec<(String, String)> {
        edits
            .iter()
            .map(|e| (e.from.clone(), e.to.clone()))
            .collect()
    }

    #[test]
    fn learns_a_latin_word_replacement_as_whole_words() {
        let edits = learn_edits(
            &chars("hello cartoon world, meet the team"),
            &chars("hello Cartune world, meet the team"),
        );
        assert_eq!(pairs(&edits), vec![("cartoon".into(), "Cartune".into())]);
    }

    #[test]
    fn learns_cjk_to_latin_term() {
        let edits = learn_edits(
            &chars("我們用卡通的系統跑"),
            &chars("我們用Cartune的系統跑"),
        );
        assert_eq!(pairs(&edits), vec![("卡通".into(), "Cartune".into())]);
    }

    #[test]
    fn joins_cjk_hunks_split_by_a_shared_character() {
        let edits = learn_edits(&chars("去找魚寶媽聊聊"), &chars("去找漁寶馬聊聊"));
        assert_eq!(pairs(&edits), vec![("魚寶媽".into(), "漁寶馬".into())]);
    }

    #[test]
    fn single_character_slips_learn_with_their_neighbour() {
        let edits = learn_edits(&chars("明天開會討論一下"), &chars("明天開匯討論一下"));
        assert_eq!(pairs(&edits), vec![("開會".into(), "開匯".into())]);
        let edits = learn_edits(&chars("池典裡有這個字"), &chars("辭典裡有這個字"));
        assert_eq!(pairs(&edits), vec![("池典".into(), "辭典".into())]);
    }

    #[test]
    fn wording_changes_are_not_learned() {
        assert!(learn_edits(&chars("明天開會討論一下"), &chars("明天會議討論一下")).is_empty());
        assert!(learn_edits(
            &chars("請把 api 的 pr 先合併"),
            &chars("請把 API 的 PR 先合併")
        )
        .is_empty());
    }

    #[test]
    fn learns_two_separate_fixes() {
        let edits = learn_edits(
            &chars("我們今天下午會用 ngnix 和 reddis 來部署這個服務和資料庫"),
            &chars("我們今天下午會用 Nginx 和 Redis 來部署這個服務和資料庫"),
        );
        assert_eq!(
            pairs(&edits),
            vec![
                ("ngnix".into(), "Nginx".into()),
                ("reddis".into(), "Redis".into())
            ]
        );
    }

    #[test]
    fn ignores_text_typed_after_the_span() {
        let edits = learn_edits(
            &chars("meet the team"),
            &chars("meet the team and then lunch"),
        );
        assert!(edits.is_empty());
    }

    #[test]
    fn ignores_deleted_words_and_full_rewrites() {
        assert!(learn_edits(
            &chars("meet the whole team today"),
            &chars("meet the team today")
        )
        .is_empty());
        assert!(learn_edits(
            &chars("we should meet the team today"),
            &chars("let us all gather tomorrow instead")
        )
        .is_empty());
    }

    #[test]
    fn short_spans_skip_the_ratio_rule_but_long_rewrites_still_fail_it() {
        assert_eq!(
            pairs(&learn_edits(&chars("用卡通"), &chars("用Cartune"))),
            vec![("卡通".into(), "Cartune".into())]
        );
        let long = "今天的會議要討論三件事情，第一是預算，第二是人力，第三是時程安排";
        let rewritten = "今天的聚會要討論三件事情，第一是經費，第二是人員，第三是時間安排";
        assert!(learn_edits(&chars(long), &chars(rewritten)).len() <= MAX_HUNKS);
        let heavy = "今天的會議要討論三件事情";
        let heavy_rewrite = "明天的聚會要處理四件工作";
        assert!(learn_edits(&chars(heavy), &chars(heavy_rewrite)).is_empty());
    }

    #[test]
    fn ignores_punctuation_only_changes() {
        assert!(learn_edits(&chars("你好，世界"), &chars("你好。世界")).is_empty());
        assert!(learn_edits(&chars("hello world"), &chars("hello, world")).is_empty());
    }

    #[test]
    fn ignores_sentence_sized_replacements() {
        let edits = learn_edits(
            &chars("第一，先做這個。第二，再做那個。"),
            &chars("第一，先做這個。第二，然後我們改成完全不同的做法。"),
        );
        assert!(edits.is_empty());
    }

    #[test]
    fn learnable_pairs_are_recognition_slips_only() {
        for (from, to) in [
            ("池點", "辭典"),
            ("卡通", "Cartune"),
            ("尚寧", "Sunny"),
            ("T 塔", "TITA"),
            ("腿好", "推好人"),
            ("RockfatherX", "GrokBot X API"),
            ("Yuma", "魚媽"),
        ] {
            assert!(is_learnable_pair(from, to), "{from} -> {to}");
        }
        for (from, to) in [
            ("你", "您"),
            ("就還", "才"),
            ("要再", "需要"),
            ("援", "持"),
            ("崔", "Threads"),
            ("大", "魚寶魚媽知名"),
            ("魚寶魚媽知名", "魚媽魚寶之名"),
            ("model", "Model"),
            ("哎呦", "車友"),
            ("自由", "支援"),
            ("這個", "那個"),
            ("mode l", "model"),
            ("open type less", "OpenTypeless"),
        ] {
            assert!(!is_learnable_pair(from, to), "{from} -> {to}");
        }
    }

    #[test]
    fn anchor_uses_the_caret_when_it_lines_up() {
        let value = chars("note: hello cartoon world");
        let anchor = anchor_from_caret(&value, Some(value.len()), "hello cartoon world").unwrap();
        assert_eq!(anchor.before, chars("note: "));
        assert_eq!(anchor.span, chars("hello cartoon world"));
        assert!(anchor.after.is_empty());
    }

    #[test]
    fn anchor_accepts_autocorrected_span_and_falls_back_to_search() {
        let value = chars("x -- hello cartoon world — ok");
        // Caret unknown: unique search.
        let anchor = anchor_from_caret(&value, None, "hello cartoon world").unwrap();
        assert_eq!(anchor.after, chars(" — ok"));
        // Caret lines up with a lightly autocorrected span: the window keeps the
        // inserted length, so a trailing autocorrect shifts it by a character,
        // which the context anchors absorb.
        let value = chars("Hello cartoon world.");
        let anchor = anchor_from_caret(&value, Some(value.len()), "hello cartoon world").unwrap();
        assert_eq!(anchor.span, chars("ello cartoon world."));
        assert_eq!(anchor.before, chars("H"));
        // Ambiguous without a caret.
        assert!(anchor_from_caret(&chars("ab ab"), None, "ab").is_none());
    }

    #[test]
    fn locate_span_follows_the_context_after_more_typing() {
        let anchor = SpanAnchor {
            before: chars("note: "),
            span: chars("hello cartoon world"),
            after: Vec::new(),
        };
        let later = chars("note: hello Cartune world and more");
        let (s, e) = locate_span(&anchor, &later).unwrap();
        assert_eq!(
            later[s..e].iter().collect::<String>(),
            "hello Cartune world and more"
        );
        let edits = learn_edits(&anchor.span, &later[s..e]);
        assert_eq!(pairs(&edits), vec![("cartoon".into(), "Cartune".into())]);
        assert!(locate_span(&anchor, &chars("completely different")).is_none());
    }

    #[test]
    fn utf16_offsets_map_to_char_indexes() {
        let value = chars("a😀b");
        assert_eq!(utf16_offset_to_char_index(&value, 0), Some(0));
        assert_eq!(utf16_offset_to_char_index(&value, 1), Some(1));
        assert_eq!(utf16_offset_to_char_index(&value, 3), Some(2));
        assert_eq!(utf16_offset_to_char_index(&value, 4), Some(3));
        assert_eq!(utf16_offset_to_char_index(&value, 2), None);
    }
}
