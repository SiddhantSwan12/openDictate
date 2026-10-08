//! Port of BetterWispr `VocabularyProcessor.swift`.

use fancy_regex::{escape, Regex};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyEntry {
    pub id: String,
    pub phrase: String,
    pub replacement: String,
    /// True when the entry came from a correction the user made, not typed in Vocabulary.
    #[serde(default)]
    pub learned: bool,
}

impl VocabularyEntry {
    pub fn new(phrase: &str, replacement: &str, learned: bool) -> Self {
        Self { id: uuid::Uuid::new_v4().to_string(), phrase: phrase.into(), replacement: replacement.into(), learned }
    }
}

/// One pass avoids cascading replacements; Unicode boundaries preserve words like café.
pub fn apply(entries: &[VocabularyEntry], text: &str) -> String {
    corrected(entries, text).0
}

/// Applies vocabulary and counts the matches whose spelling actually changed.
pub fn corrected(entries: &[VocabularyEntry], text: &str) -> (String, usize) {
    let mut entries: Vec<&VocabularyEntry> = entries.iter().filter(|e| !e.phrase.trim().is_empty()).collect();
    if entries.is_empty() {
        return (text.trim().to_string(), 0);
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.phrase.chars().count()));
    let alternatives = entries.iter().map(|e| escape(&e.phrase).into_owned()).collect::<Vec<_>>().join("|");
    let pattern = format!(r"(?i)(?<![\p{{L}}\p{{M}}\p{{N}}_])(?:{alternatives})(?![\p{{L}}\p{{M}}\p{{N}}_])");
    let Ok(regex) = Regex::new(&pattern) else {
        return (text.to_string(), 0);
    };
    let mut output = String::with_capacity(text.len());
    let mut last = 0;
    let mut fixes = 0;
    for m in regex.find_iter(text).flatten() {
        let phrase = m.as_str();
        let lower = phrase.to_lowercase();
        let Some(entry) = entries.iter().find(|e| e.phrase.to_lowercase() == lower) else { continue };
        let replacement = if entry.replacement.is_empty() { &entry.phrase } else { &entry.replacement };
        if replacement != phrase {
            fixes += 1;
        }
        output.push_str(&text[last..m.start()]);
        output.push_str(replacement);
        last = m.end();
    }
    output.push_str(&text[last..]);
    (output.trim().to_string(), fixes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(phrase: &str, replacement: &str) -> VocabularyEntry {
        VocabularyEntry::new(phrase, replacement, false)
    }

    #[test]
    fn uses_whole_words_and_never_cascades() {
        let entries = [
            entry("better whisper", "BetterWispr"),
            entry("BetterWispr", "wrong"),
            entry("café", "Café"),
            entry("cat", "$1\\dog"),
        ];
        assert_eq!(
            apply(&entries, "better whisper at café, not cafés. cat scatter"),
            "BetterWispr at Café, not cafés. $1\\dog scatter"
        );
        assert_eq!(apply(&[entry("ह", "wrong")], "हिंदी"), "हिंदी");
    }

    #[test]
    fn counts_only_changed_spellings() {
        let entries = [entry("cardic", "Kartik"), entry("BetterWispr", "")];
        assert_eq!(
            corrected(&entries, "cardic uses betterwispr and BetterWispr"),
            ("Kartik uses BetterWispr and BetterWispr".to_string(), 2)
        );
    }
}
