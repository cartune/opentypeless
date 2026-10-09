//! "音同不同": does a corrected Chinese term sound like what was dictated?
//! Speech recognition confuses homophones and near-homophones; it does not
//! swap a word for a synonym or reorder a phrase. So an edit is only taken as
//! a recognition fix when the two sides read alike syllable by syllable.

use pinyin::ToPinyinMulti;

/// Share of aligned syllables that must sound alike.
const MIN_MATCH_RATIO: f32 = 0.6;

/// One character's possible readings, toneless and with Taiwan-typical
/// confusions merged (zh/z, ch/c, sh/s, n/l, in/ing, en/eng, an/ang, ü/u).
fn readings(character: char) -> Option<Vec<String>> {
    let multi = character.to_pinyin_multi()?;
    let mut out: Vec<String> = multi
        .into_iter()
        .map(|reading| normalise(reading.plain()))
        .collect();
    out.sort();
    out.dedup();
    Some(out)
}

fn normalise(plain: &str) -> String {
    let lower = plain.to_ascii_lowercase().replace(['ü', 'v'], "u");
    let (initial, final_part) = split_syllable(&lower);
    let initial = match initial {
        "zh" => "z",
        "ch" => "c",
        "sh" => "s",
        "l" => "n",
        other => other,
    };
    let final_part = match final_part {
        "ing" => "in",
        "eng" => "en",
        "ang" => "an",
        "iang" => "ian",
        "uang" => "uan",
        other => other,
    };
    format!("{initial}{final_part}")
}

fn split_syllable(syllable: &str) -> (&str, &str) {
    for initial in [
        "zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x",
        "r", "z", "c", "s", "y", "w",
    ] {
        if let Some(rest) = syllable.strip_prefix(initial) {
            if !rest.is_empty() {
                return (initial, rest);
            }
        }
    }
    ("", syllable)
}

/// Readings of every Chinese character in `text`, in order. Characters
/// without a reading (punctuation, Latin letters) are skipped.
fn syllables(text: &str) -> Vec<Vec<String>> {
    text.chars().filter_map(readings).collect()
}

fn alike(a: &[String], b: &[String]) -> bool {
    a.iter().any(|reading| b.contains(reading))
}

fn matches_aligned(a: &[Vec<String>], b: &[Vec<String>]) -> usize {
    a.iter().zip(b).filter(|(x, y)| alike(x, y)).count()
}

/// Best positional match count when one side has one syllable more: try
/// dropping each syllable of the longer side.
fn matches_with_one_gap(short: &[Vec<String>], long: &[Vec<String>]) -> usize {
    (0..long.len())
        .map(|skip| {
            let trimmed: Vec<Vec<String>> = long
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != skip)
                .map(|(_, reading)| reading.clone())
                .collect();
            matches_aligned(short, &trimmed)
        })
        .max()
        .unwrap_or(0)
}

fn is_reorder(a: &[Vec<String>], b: &[Vec<String>]) -> bool {
    let mut first_a: Vec<&String> = a.iter().filter_map(|r| r.first()).collect();
    let mut first_b: Vec<&String> = b.iter().filter_map(|r| r.first()).collect();
    first_a.sort();
    first_b.sort();
    first_a == first_b
}

/// `None` when either side has no readable Chinese character (the caller
/// falls back to a character-overlap check).
pub fn sounds_alike(from: &str, to: &str) -> Option<bool> {
    let a = syllables(from);
    let b = syllables(to);
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let (short, long) = if a.len() <= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    if long.len() - short.len() > 1 {
        return Some(false);
    }
    let matched = if long.len() == short.len() {
        let aligned = matches_aligned(short, long);
        // Same syllables in a different order is the user rephrasing
        // (魚寶魚媽 → 魚媽魚寶); recognition never swaps words around.
        if aligned < long.len() && is_reorder(short, long) {
            return Some(false);
        }
        aligned
    } else {
        matches_with_one_gap(short, long)
    };
    let needed = (long.len() as f32 * MIN_MATCH_RATIO).ceil() as usize;
    Some(matched >= needed.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognition_slips_sound_alike() {
        for (from, to) in [
            ("池點", "辭典"),
            ("卡通", "咖通"),
            ("腿好", "推好人"),
            ("脆", "翠"),
            ("知名", "之名"),
            ("開會", "開匯"),
        ] {
            assert_eq!(sounds_alike(from, to), Some(true), "{from} -> {to}");
        }
    }

    #[test]
    fn wording_changes_and_reorders_do_not() {
        for (from, to) in [
            ("你", "您"),
            ("就還", "才"),
            ("要再", "需要"),
            ("大", "魚寶魚媽知名"),
            ("魚寶魚媽知名", "魚媽魚寶之名"),
            ("哎呦", "車友"),
            ("支援", "支持"),
        ] {
            assert_eq!(sounds_alike(from, to), Some(false), "{from} -> {to}");
        }
    }

    #[test]
    fn latin_only_sides_are_unknown() {
        assert_eq!(sounds_alike("Sunny", "尚寧"), None);
        assert_eq!(sounds_alike("model", "Model"), None);
    }
}
