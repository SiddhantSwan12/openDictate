//! Port of BetterWispr `WritingStyle.swift` and `StyleFormatter.swift`.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupLevel {
    None,
    #[default]
    Light,
    Medium,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleContext {
    Personal,
    Work,
    Email,
    Other,
}

impl StyleContext {
    pub const ALL: [StyleContext; 4] = [Self::Personal, Self::Work, Self::Email, Self::Other];

    pub fn tones(self) -> &'static [StyleTone] {
        if self == Self::Personal {
            &[StyleTone::Formal, StyleTone::Casual, StyleTone::VeryCasual]
        } else {
            &[StyleTone::Formal, StyleTone::Casual, StyleTone::Excited]
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleTone {
    #[default]
    Formal,
    Casual,
    VeryCasual,
    Excited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppCategory {
    AiPrompts,
    Work,
    Personal,
    Documents,
    Email,
    Other,
}

/// Lowercased Windows executable names. Browsers and unknown apps are `Other`
/// because the website in use is not inspected.
const REGISTRY: &[(&str, AppCategory)] = &[
    ("chatgpt.exe", AppCategory::AiPrompts),
    ("claude.exe", AppCategory::AiPrompts),
    ("codex.exe", AppCategory::AiPrompts),
    ("cursor.exe", AppCategory::AiPrompts),
    ("windsurf.exe", AppCategory::AiPrompts),
    ("perplexity.exe", AppCategory::AiPrompts),
    ("slack.exe", AppCategory::Work),
    ("ms-teams.exe", AppCategory::Work),
    ("teams.exe", AppCategory::Work),
    ("zoom.exe", AppCategory::Work),
    ("webex.exe", AppCategory::Work),
    ("mattermost.exe", AppCategory::Work),
    ("whatsapp.exe", AppCategory::Personal),
    ("whatsapp.root.exe", AppCategory::Personal),
    ("telegram.exe", AppCategory::Personal),
    ("discord.exe", AppCategory::Personal),
    ("signal.exe", AppCategory::Personal),
    ("messenger.exe", AppCategory::Personal),
    ("winword.exe", AppCategory::Documents),
    ("notion.exe", AppCategory::Documents),
    ("obsidian.exe", AppCategory::Documents),
    ("notepad.exe", AppCategory::Documents),
    ("onenote.exe", AppCategory::Documents),
    ("wordpad.exe", AppCategory::Documents),
    ("outlook.exe", AppCategory::Email),
    ("olk.exe", AppCategory::Email),
    ("thunderbird.exe", AppCategory::Email),
    ("hxoutlook.exe", AppCategory::Email),
    ("superhuman.exe", AppCategory::Email),
];

impl AppCategory {
    pub fn from_app(exe: Option<&str>) -> Self {
        let Some(exe) = exe else { return Self::Other };
        let exe = exe.to_lowercase();
        REGISTRY.iter().find(|(name, _)| *name == exe).map_or(Self::Other, |(_, category)| *category)
    }

    pub fn style(self) -> StyleContext {
        match self {
            Self::Personal => StyleContext::Personal,
            Self::Work => StyleContext::Work,
            Self::Email => StyleContext::Email,
            Self::AiPrompts | Self::Documents | Self::Other => StyleContext::Other,
        }
    }
}

static GREETING: Lazy<Regex> = Lazy::new(|| Regex::new(r"\A([^\n.!?,]{1,40}),\n+(\p{L}[\p{L}'’]*)").unwrap());

/// Applies an English writing tone; formal leaves the recognized text unchanged.
pub fn apply(tone: StyleTone, text: &str) -> String {
    match tone {
        StyleTone::Formal => text.to_string(),
        StyleTone::Casual => casual(text),
        StyleTone::VeryCasual => lowercasing_sentence_starts(&casual(text)),
        StyleTone::Excited => excited(text),
    }
}

fn casual(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut kept = String::with_capacity(text.len());
    for (index, &character) in chars.iter().enumerate() {
        let next = chars.get(index + 1).copied();
        let inside_number = index > 0 && chars[index - 1].is_numeric() && next.is_some_and(char::is_numeric);
        if character == ',' && !inside_number && !next.is_some_and(|c| c == '\n' || c == '\r') {
            continue;
        }
        kept.push(character);
    }
    if let Some(period) = final_period(&kept) {
        if period + 1 == kept.len() {
            kept.remove(period);
        }
    }
    joining_greeting(&kept)
}

fn excited(text: &str) -> String {
    let Some(period) = final_period(text) else { return text.to_string() };
    let mut result = text.to_string();
    result.replace_range(period..period + 1, "!");
    result
}

/// The byte index of the period ending the last sentence, skipping ellipses,
/// abbreviations like "p.m." and decimals.
fn final_period(text: &str) -> Option<usize> {
    let end = text.rfind(['.', '!', '?'])?;
    if !text[end..].starts_with('.') {
        return None;
    }
    if text[end + 1..].chars().next().is_some_and(|c| !c.is_whitespace()) {
        return None;
    }
    let word: Vec<char> = text[..end].chars().rev().take_while(|c| !c.is_whitespace()).collect();
    match word.first() {
        Some(c) if c.is_alphabetic() || c.is_numeric() => (!word.contains(&'.')).then_some(end),
        _ => None,
    }
}

fn joining_greeting(text: &str) -> String {
    let Some(caps) = GREETING.captures(text) else { return text.to_string() };
    let greeting = caps.get(1).unwrap().as_str();
    if greeting.split(' ').filter(|s| !s.is_empty()).count() > 4 {
        return text.to_string();
    }
    let word = caps.get(2).unwrap().as_str();
    let end = caps.get(0).unwrap().end();
    format!("{greeting}, {}{}", sentence_cased(word), &text[end..])
}

fn lowercasing_sentence_starts(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());
    let mut at_sentence_start = true;
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if at_sentence_start && character.is_alphabetic() {
            let len = chars[index..].iter().take_while(|c| c.is_alphabetic() || **c == '\'' || **c == '’').count();
            let word: String = chars[index..index + len].iter().collect();
            if word.chars().skip(1).any(char::is_uppercase) {
                result.push_str(&word);
            } else {
                result.push_str(&word.to_lowercase());
            }
            at_sentence_start = false;
            index += len;
            continue;
        }
        if character.is_alphabetic() || character.is_numeric() {
            at_sentence_start = false;
        }
        if ".!?\n".contains(character) {
            at_sentence_start = true;
        }
        result.push(character);
        index += 1;
    }
    result
}

fn sentence_cased(word: &str) -> String {
    let keeps_case = word == "I"
        || word.starts_with("I'")
        || word.starts_with("I’")
        || word.chars().skip(1).any(char::is_uppercase);
    if keeps_case { word.to_string() } else { word.to_lowercase() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use StyleTone::*;

    #[test]
    fn personal_tones_match_their_samples() {
        let spoken = "Hey, are you free for lunch tomorrow? Let's do 12 if that works for you.";
        assert_eq!(apply(Formal, spoken), spoken);
        assert_eq!(apply(Casual, spoken), "Hey are you free for lunch tomorrow? Let's do 12 if that works for you");
        assert_eq!(apply(VeryCasual, spoken), "hey are you free for lunch tomorrow? let's do 12 if that works for you");
    }

    #[test]
    fn work_and_email_tones_match_their_samples() {
        let work = "Hey, if you're free, let's chat about the great results.";
        assert_eq!(apply(Casual, work), "Hey if you're free let's chat about the great results");
        assert_eq!(apply(Excited, work), "Hey, if you're free, let's chat about the great results!");
        let email = "Hi Alex,\n\nIt was great talking with you today. Looking forward to our next chat.\n\nBest,\nMary";
        assert_eq!(apply(Formal, email), email);
        assert_eq!(
            apply(Casual, email),
            "Hi Alex, it was great talking with you today. Looking forward to our next chat.\n\nBest,\nMary"
        );
        assert_eq!(
            apply(Excited, email),
            "Hi Alex,\n\nIt was great talking with you today. Looking forward to our next chat!\n\nBest,\nMary"
        );
        let other = "So far, I am enjoying the new workout routine.\n\nI am excited for tomorrow's workout, especially after a full night of rest.";
        assert_eq!(
            apply(Casual, other),
            "So far I am enjoying the new workout routine.\n\nI am excited for tomorrow's workout especially after a full night of rest"
        );
        assert!(apply(Excited, other).ends_with("full night of rest!"));
    }

    #[test]
    fn tones_keep_numbers_names_and_ellipses() {
        assert_eq!(apply(Casual, "It costs 1,200 dollars, sadly."), "It costs 1,200 dollars sadly");
        assert_eq!(
            apply(VeryCasual, "I met iPhone fans. NASA called. Kartik said hi"),
            "i met iPhone fans. NASA called. kartik said hi"
        );
        assert_eq!(apply(Excited, "Wait for it..."), "Wait for it...");
        assert_eq!(apply(Excited, "Is it done?"), "Is it done?");
        assert_eq!(apply(Casual, "See you at 5 p.m."), "See you at 5 p.m.");
        assert_eq!(apply(Excited, "Version 2.0"), "Version 2.0");
        assert_eq!(apply(Casual, "Hi team,\n\nI shipped it."), "Hi team, I shipped it");
    }

    #[test]
    fn apps_map_to_style_contexts() {
        assert_eq!(AppCategory::from_app(Some("slack.exe")).style(), StyleContext::Work);
        assert_eq!(AppCategory::from_app(Some("WhatsApp.exe")).style(), StyleContext::Personal);
        assert_eq!(AppCategory::from_app(Some("OUTLOOK.EXE")).style(), StyleContext::Email);
        assert_eq!(AppCategory::from_app(Some("ChatGPT.exe")), AppCategory::AiPrompts);
        assert_eq!(AppCategory::from_app(Some("ChatGPT.exe")).style(), StyleContext::Other);
        assert_eq!(AppCategory::from_app(Some("chrome.exe")), AppCategory::Other);
        assert_eq!(AppCategory::from_app(None), AppCategory::Other);
    }
}
