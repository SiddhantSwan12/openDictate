//! Port of BetterWispr `CorrectionLearner.swift` and the pure parts of `CorrectionWatcher.swift`.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct LearnedCorrection {
    pub heard: String,
    pub corrected: String,
}

impl LearnedCorrection {
    fn new(heard: &str, corrected: &str) -> Self {
        Self { heard: heard.into(), corrected: corrected.into() }
    }
}

static COMMON_WORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    "the and for not with you this but his from they say her she will one all would
    there their what out about who get which when make can like time just him know
    take into year your good some could them see other than then now look only come
    over think also back after use two how our work first well way even new want
    because any these give day most are was were been has had did does said went
    made got came took saw knew thought where why here very much many still too again
    off down never every own same another both each few more less last next while
    before through under between should might must being have that its yes okay"
        .split_whitespace()
        .collect()
});

static EDGES: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[^\p{L}\p{M}\p{N}_]+|[^\p{L}\p{M}\p{N}_]+$").unwrap());

/// Returns word-level fixes the user made to dictated text, skipping rewrites and ordinary word swaps.
pub fn corrections(original: &str, edited: &str) -> Vec<LearnedCorrection> {
    let before = tokenize(original);
    let after = tokenize(edited);
    if before.is_empty() || after.is_empty() || before == after {
        return Vec::new();
    }
    let substitutions = substitutions(&before, &after);
    if substitutions.len() > (before.len() / 2).max(1) {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    substitutions
        .into_iter()
        .filter(|change| {
            let inserted = seen.insert(change.corrected.to_lowercase());
            inserted && is_vocabulary(change)
        })
        .collect()
}

/// Whether the heard phrase is an everyday word that must not be rewritten everywhere.
pub fn is_common(phrase: &str) -> bool {
    COMMON_WORDS.contains(phrase.to_lowercase().as_str())
}

pub fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| EDGES.replace_all(w, "").into_owned())
        .filter(|w| !w.is_empty())
        .collect()
}

fn is_vocabulary(change: &LearnedCorrection) -> bool {
    let heard = change.heard.to_lowercase();
    let corrected = change.corrected.to_lowercase();
    if !corrected.chars().any(char::is_alphabetic) {
        return false;
    }
    if heard == corrected {
        return change.corrected.chars().skip(1).any(char::is_uppercase);
    }
    if change.corrected.chars().count() < 3 && !change.corrected.chars().any(char::is_uppercase) {
        return false;
    }
    if COMMON_WORDS.contains(corrected.as_str()) {
        return false;
    }
    let compact_heard: Vec<char> = heard.chars().filter(|c| !c.is_whitespace()).collect();
    let compact_corrected: Vec<char> = corrected.chars().filter(|c| !c.is_whitespace()).collect();
    let distance = edit_distance(&compact_heard, &compact_corrected);
    distance as f64 / compact_heard.len().max(compact_corrected.len()) as f64 <= 0.65
}

fn substitutions(before: &[String], after: &[String]) -> Vec<LearnedCorrection> {
    let (m, n) = (before.len(), after.len());
    let mut lcs = vec![vec![0usize; n + 1]; m + 1];
    for i in (0..m).rev() {
        for j in (0..n).rev() {
            lcs[i][j] = if before[i] == after[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let mut result = Vec::new();
    let mut removed: Vec<&str> = Vec::new();
    let mut added: Vec<&str> = Vec::new();
    let mut flush = |removed: &mut Vec<&str>, added: &mut Vec<&str>| {
        if (1..=3).contains(&removed.len()) && (1..=3).contains(&added.len()) {
            result.push(LearnedCorrection::new(&removed.join(" "), &added.join(" ")));
        }
        removed.clear();
        added.clear();
    };
    let (mut i, mut j) = (0, 0);
    while i < m || j < n {
        if i < m && j < n && before[i] == after[j] {
            flush(&mut removed, &mut added);
            i += 1;
            j += 1;
        } else if j < n && (i == m || lcs[i][j + 1] > lcs[i + 1][j]) {
            added.push(&after[j]);
            j += 1;
        } else {
            removed.push(&before[i]);
            i += 1;
        }
    }
    flush(&mut removed, &mut added);
    result
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, y) in b.iter().enumerate() {
            let value = if x == y { previous[j] } else { 1 + previous[j].min(previous[j + 1]).min(current[j]) };
            current.push(value);
        }
        previous = current;
    }
    previous[b.len()]
}

/// Returns the inserted text with the user's change applied, or None when the change falls outside it.
pub fn edit_of(inserted: &str, before: &str, after: &str) -> Option<String> {
    let byte_start = before.rfind(inserted)?;
    let old: Vec<char> = before.chars().collect();
    let new: Vec<char> = after.chars().collect();
    let shortest = old.len().min(new.len());
    let mut prefix = 0;
    while prefix < shortest && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < shortest - prefix && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix] {
        suffix += 1;
    }
    let start = before[..byte_start].chars().count();
    let end = start + inserted.chars().count();
    if prefix < start || old.len() - suffix > end || old.len() - suffix <= prefix {
        return None;
    }
    let local: Vec<char> = inserted.chars().collect();
    let mut result: String = local[..prefix - start].iter().collect();
    result.extend(&new[prefix..new.len() - suffix]);
    result.extend(&local[old.len() - suffix - start..]);
    Some(result)
}

/// Decides when text in a field polled every half second is final enough to learn from.
pub struct SettledText {
    last: String,
    polls: u32,
}

impl SettledText {
    pub fn new(value: &str) -> Self {
        Self { last: value.to_string(), polls: 1 }
    }

    /// Returns text left unchanged for two seconds, or the text that was in the
    /// field for at least half a second before it emptied.
    pub fn observe(&mut self, value: &str) -> Option<String> {
        if value.is_empty() {
            return (self.polls >= 2).then(|| self.last.clone());
        }
        if value == self.last {
            self.polls += 1;
        } else {
            self.last = value.to_string();
            self.polls = 1;
        }
        (self.polls == 5).then(|| value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocabulary::{self, VocabularyEntry};

    #[test]
    fn keeps_vocabulary_fixes_only() {
        assert_eq!(corrections("Thanks kumr, see you", "Thanks Kumar, see you"), [LearnedCorrection::new("kumr", "Kumar")]);
        assert_eq!(
            corrections("try assist able today", "try Assistable today"),
            [LearnedCorrection::new("assist able", "Assistable")]
        );
        assert_eq!(corrections("ask kv about it", "ask KV about it"), [LearnedCorrection::new("kv", "KV")]);
        assert!(corrections("why is it late", "what is it late").is_empty());
        assert!(corrections("hello there", "hello there!").is_empty());
        assert!(corrections("send the report today", "please call me tomorrow instead").is_empty());
    }

    #[test]
    fn one_fix_to_real_parakeet_output_corrects_the_next_dictation() {
        let heard = "I dictate with better whisper and then paste it into cloud code. Open the superbase dashboard and check the post hog events.";
        let fixed = "I dictate with BetterWispr and then paste it into Claude Code. Open the Supabase dashboard and check the PostHog events.";
        let learned = corrections(heard, fixed);
        let names: Vec<&str> = learned.iter().map(|c| c.corrected.as_str()).collect();
        assert_eq!(names, ["BetterWispr", "Claude Code", "Supabase", "PostHog"]);
        let entries: Vec<VocabularyEntry> =
            learned.iter().map(|c| VocabularyEntry::new(&c.heard, &c.corrected, true)).collect();
        assert_eq!(vocabulary::corrected(&entries, heard), (fixed.to_string(), 4));
        let unrelated = "Store it in the cloud and review the code.";
        assert_eq!(vocabulary::apply(&entries, unrelated), unrelated);
    }

    #[test]
    fn watcher_edit_stays_inside_the_dictation() {
        assert_eq!(edit_of("ping kv now", "Hi. ping kv now", "Hi. ping KV now").as_deref(), Some("ping KV now"));
        assert_eq!(edit_of("ping kv now", "Hi. ping kv now", "Hey. ping kv now"), None);
        assert_eq!(edit_of("ping kv now", "ping kv now", "ping kv now please"), None);
    }

    #[test]
    fn watcher_learns_settled_or_sent_text_only() {
        let mut typing = SettledText::new("cloud code");
        assert!(["Claude Co", "Claude Co", "Claude Co", "Claude Code"].iter().all(|v| typing.observe(v).is_none()));
        assert!(["Claude Code", "Claude Code", "Claude Code"].iter().all(|v| typing.observe(v).is_none()));
        assert_eq!(typing.observe("Claude Code").as_deref(), Some("Claude Code"));
        assert_eq!(typing.observe("Claude Code"), None);

        let mut sent = SettledText::new("cloud code");
        assert_eq!(sent.observe("Claude Code"), None);
        assert_eq!(sent.observe("Claude Code"), None);
        assert_eq!(sent.observe("").as_deref(), Some("Claude Code"));

        let mut rushed = SettledText::new("cloud code");
        assert_eq!(rushed.observe("Claude Cod"), None);
        assert_eq!(rushed.observe(""), None);
    }
}
