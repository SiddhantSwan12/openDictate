//! Port of BetterWispr `TranscriptCleaner.swift`.

use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Clone, Debug)]
struct Token {
    word: String,
    trailing: String,
}

const FILLERS: &[&str] = &["uh", "uhh", "uhm", "um", "umm", "er", "erm", "hm", "hmm", "mm", "mmm"];
const KEPT_DOUBLES: &[&str] = &["that", "had", "is", "very", "really", "long", "bye", "no", "ha"];
const NUMBER_WORDS: &[&str] = &["zero", "oh", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten"];
const REPAIR_CUES: &[&[&str]] = &[&["sorry"], &["no"], &["wait"], &["oops"], &["actually"], &["i", "mean"]];
const CORRECTING_CUES: &[&[&str]] = &[&["no"], &["i", "mean"]];
const SUBJECT_PRONOUNS: &[&str] = &["i", "we", "you", "he", "she", "it", "they"];
const REPAIR_REACH: usize = 4;
const SENTENCE_ENDERS: &[char] = &['.', '?', '!'];
const TERMINATORS: &[char] = &['.', '?', '!', '…'];
const CLAUSE_BREAKS: &[char] = &['.', '?', '!', '…', ','];

static WORD: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\p{L}\p{M}\p{N}_'’-]+").unwrap());
static SPACES: Lazy<Regex> = Lazy::new(|| Regex::new(r" {2,}").unwrap());

fn has_any(s: &str, set: &[char]) -> bool {
    s.chars().any(|c| set.contains(&c))
}

fn is_blank(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

/// Removes English filled pauses, set-off "you know" and unpunctuated stutters; other languages are only trimmed.
pub fn clean(text: &str, language: Option<&str>) -> String {
    let matches: Vec<_> = WORD.find_iter(text).collect();
    let tokens: Vec<Token> = matches
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let end = matches.get(i + 1).map_or(text.len(), |next| next.start());
            Token { word: m.as_str().to_string(), trailing: text[m.end()..end].to_string() }
        })
        .collect();
    let spoken = tokens
        .iter()
        .map(|t| t.word.as_str())
        .filter(|w| !FILLERS.contains(&w.to_lowercase().as_str()))
        .collect::<Vec<_>>()
        .join(" ");
    if !is_english(&spoken, language) {
        return text.trim().to_string();
    }
    let leading = &text[..matches.first().map_or(text.len(), |m| m.start())];
    let rebuilt: String = std::iter::once(leading.to_string())
        .chain(destutter(drop_repairs(drop_fillers(&tokens))).into_iter().map(|t| t.word + &t.trailing))
        .collect();
    let mut result = SPACES.replace_all(&rebuilt, " ").trim().to_string();
    if result.ends_with(',') {
        result.pop();
    }
    result.trim().to_string()
}

/// Uses the chosen language, or detects it when the language is automatic; undetectable text counts as English.
pub fn is_english(text: &str, language: Option<&str>) -> bool {
    match language {
        Some(code) => base_language(code) == "en",
        None => crate::detect_language(text).map_or(true, |code| code == "en"),
    }
}

/// "en-US" and "en_GB" become "en".
pub fn base_language(code: &str) -> String {
    code.split(['-', '_']).next().unwrap_or("").to_lowercase()
}

fn capitalize_first(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn drop_fillers(tokens: &[Token]) -> Vec<Token> {
    let mut kept: Vec<Token> = Vec::new();
    let mut capitalize_next = false;
    let mut to_skip = 0;
    for (index, token) in tokens.iter().enumerate() {
        if to_skip > 0 {
            to_skip -= 1;
            continue;
        }
        let removed = filler_length(tokens, index, kept.last().map(|t| t.trailing.as_str()));
        if removed > 0 {
            to_skip = removed - 1;
            let removed_trailing = tokens[index + to_skip].trailing.clone();
            let Some(last) = kept.last_mut() else {
                capitalize_next = true;
                continue;
            };
            let previous = last.trailing.clone();
            capitalize_next = has_any(&previous, SENTENCE_ENDERS);
            if has_any(&removed_trailing, TERMINATORS) && !has_any(&previous, TERMINATORS) {
                last.trailing = removed_trailing;
            } else if removed > 1 && previous.contains(',') && removed_trailing.contains(',') {
                last.trailing = previous.replace(',', "");
            }
            continue;
        }
        let mut token = token.clone();
        if capitalize_next && token.word == token.word.to_lowercase() {
            token.word = capitalize_first(&token.word);
        }
        capitalize_next = false;
        kept.push(token);
    }
    kept
}

fn filler_length(tokens: &[Token], index: usize, previous: Option<&str>) -> usize {
    let word = tokens[index].word.to_lowercase();
    let follows_number = index > 0 && tokens[index - 1].word.chars().all(char::is_numeric);
    if FILLERS.contains(&word.as_str()) && !(word == "mm" && follows_number) {
        return 1;
    }
    if is_set_off_you_know(tokens, index, previous) { 2 } else { 0 }
}

fn is_set_off_you_know(tokens: &[Token], index: usize, previous: Option<&str>) -> bool {
    if index + 1 >= tokens.len()
        || tokens[index].word.to_lowercase() != "you"
        || tokens[index + 1].word.to_lowercase() != "know"
        || !is_blank(&tokens[index].trailing)
    {
        return false;
    }
    let following = &tokens[index + 1].trailing;
    let set_off_before = previous.map_or(true, |p| has_any(p, CLAUSE_BREAKS));
    let set_off_after =
        !following.contains('?') && (index + 2 == tokens.len() || has_any(following, CLAUSE_BREAKS));
    set_off_before && set_off_after
}

/// Turns "to Pune, sorry, no, to Delhi" into "to Delhi" when the repair restarts on a recent non-pronoun word.
fn drop_repairs(mut tokens: Vec<Token>) -> Vec<Token> {
    let mut i = 1;
    while i < tokens.len() {
        let window_start = i.saturating_sub(REPAIR_REACH);
        let sentence_start = (window_start..i)
            .rev()
            .find(|&j| has_any(&tokens[j].trailing, TERMINATORS))
            .map_or(window_start, |j| j + 1);
        let found = if tokens[i - 1].trailing.contains(',') {
            repair_onset(&tokens, i).and_then(|onset| {
                let target = tokens[onset].word.to_lowercase();
                (sentence_start..i)
                    .rev()
                    .find(|&j| tokens[j].word.to_lowercase() == target)
                    .filter(|&anchor| {
                        let lower = tokens[anchor].word.to_lowercase();
                        let stem: String = lower.chars().take_while(|&c| c != '\'' && c != '’').collect();
                        !SUBJECT_PRONOUNS.contains(&stem.as_str())
                    })
                    .map(|anchor| (anchor, onset))
            })
        } else {
            None
        };
        let Some((anchor, onset)) = found else {
            i += 1;
            continue;
        };
        tokens[anchor].trailing = tokens[onset].trailing.clone();
        tokens.drain(anchor + 1..=onset);
        i = anchor + 1;
    }
    tokens
}

/// The first repair word after a cue that says "no" or "I mean", set off by a comma unless the cue is compound.
fn repair_onset(tokens: &[Token], start: usize) -> Option<usize> {
    let mut end = start;
    let mut cues: Vec<&[&str]> = Vec::new();
    while let Some(cue) = REPAIR_CUES.iter().find(|cue| {
        end + cue.len() <= tokens.len()
            && cue.iter().enumerate().all(|(k, w)| tokens[end + k].word.to_lowercase() == *w)
    }) {
        cues.push(cue);
        end += cue.len();
    }
    let ok = end > start
        && end < tokens.len()
        && cues.iter().any(|c| CORRECTING_CUES.contains(c))
        && (cues.len() > 1 || tokens[end - 1].trailing.contains(','))
        && !tokens[start..end].iter().any(|t| has_any(&t.trailing, TERMINATORS));
    ok.then_some(end)
}

fn destutter(mut tokens: Vec<Token>) -> Vec<Token> {
    let mut i = 0;
    while i < tokens.len() {
        let Some(n) = [3, 2, 1].into_iter().find(|&n| repeats(&tokens, i, n)) else {
            i += 1;
            continue;
        };
        tokens[i + n - 1].trailing = tokens[i + 2 * n - 1].trailing.clone();
        tokens.drain(i + n..i + 2 * n);
    }
    tokens
}

fn repeats(tokens: &[Token], i: usize, n: usize) -> bool {
    if i + 2 * n > tokens.len() {
        return false;
    }
    let span = &tokens[i..i + 2 * n];
    (0..n).all(|k| tokens[i + k].word.to_lowercase() == tokens[i + n + k].word.to_lowercase())
        && span[..span.len() - 1].iter().all(|t| is_blank(&t.trailing))
        && !span.iter().any(|t| is_protected(&t.word.to_lowercase(), n == 1))
}

fn is_protected(word: &str, single: bool) -> bool {
    word.chars().any(char::is_numeric)
        || NUMBER_WORDS.contains(&word)
        || (single && KEPT_DOUBLES.contains(&word))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en(s: &str) -> String {
        clean(s, Some("en"))
    }

    #[test]
    fn drops_mid_sentence_fillers_and_keeps_preceding_punctuation() {
        assert_eq!(en("The tests passed, uh, so merge it um now"), "The tests passed, so merge it now");
        assert_eq!(en("Rename the file uh, then reload the hmm window"), "Rename the file then reload the window");
        assert_eq!(en("and uh uh we can build um the export"), "and we can build the export");
    }

    #[test]
    fn capitalizes_after_sentence_initial_filler() {
        assert_eq!(en("Can you check the logs? um yes, and the tests too."), "Can you check the logs? Yes, and the tests too.");
        assert_eq!(en("uh iPhone builds are slow"), "iPhone builds are slow");
    }

    #[test]
    fn moves_terminal_punctuation_from_filler() {
        assert_eq!(en("We are done for today uh."), "We are done for today.");
        assert_eq!(en("Is the build ready um?"), "Is the build ready?");
    }

    #[test]
    fn drops_mm_fillers() {
        assert_eq!(en("mm I think so"), "I think so");
        assert_eq!(en("Mm, that works."), "That works.");
        assert_eq!(en("We could, mmm, try it"), "We could, try it");
        assert_eq!(en("mm-hmm, that sounds right"), "mm-hmm, that sounds right");
        assert_eq!(en("Cut it to 5 mm, mm, thanks"), "Cut it to 5 mm, thanks");
    }

    #[test]
    fn drops_you_know_only_when_set_off_on_both_sides() {
        assert_eq!(en("It was, you know, fine."), "It was fine.");
        assert_eq!(en("You know, I think so."), "I think so.");
        assert_eq!(en("That is what I want, you know."), "That is what I want.");
        assert_eq!(en("It works. You know, it is fast."), "It works. It is fast.");
        assert_eq!(en("That is what I want, you know"), "That is what I want");
    }

    #[test]
    fn keeps_you_know_that_is_not_set_off() {
        assert_eq!(en("You know the answer."), "You know the answer.");
        assert_eq!(en("Do you know, honestly?"), "Do you know, honestly?");
        assert_eq!(en("If you know, tell me."), "If you know, tell me.");
        assert_eq!(en("Well, you know what I mean."), "Well, you know what I mean.");
        assert_eq!(en("It is hard, you know?"), "It is hard, you know?");
        assert_eq!(en("Do you know?"), "Do you know?");
        assert_eq!(en("Tuesday, I mean, Wednesday"), "Tuesday, I mean, Wednesday");
        assert_eq!(clean("Du weißt, you know, es ist gut.", Some("de")), "Du weißt, you know, es ist gut.");
    }

    #[test]
    fn resolves_self_corrections_that_repeat_a_word() {
        assert_eq!(
            en("So let's say I say this sentence uh I want to go to uh Mdabad, sorry, no to Dilli. Are we processing this properly"),
            "So let's say I say this sentence I want to go to Dilli. Are we processing this properly"
        );
        assert_eq!(en("I want to go to Ahmedabad, sorry, no, to Delhi."), "I want to go to Delhi.");
        assert_eq!(en("Let's meet at 5, no wait, at 6"), "Let's meet at 6");
        assert_eq!(en("Send it to Sam, no, send it to Alex."), "Send it to Alex.");
        assert_eq!(en("Book the window seat, I mean, the aisle seat"), "Book the aisle seat");
    }

    #[test]
    fn keeps_correction_lookalikes() {
        for s in [
            "I said yes to the plan, no to the budget.",
            "I can't make it, sorry, I have a meeting.",
            "Should we go, no, we should stay.",
            "Is it at 5? No, at 6.",
            "Thanks for coming, sorry for the wait.",
            "Go to Ahmedabad, sorry, no, Delhi",
        ] {
            assert_eq!(en(s), s);
        }
    }

    #[test]
    fn removes_repeated_phrases() {
        assert_eq!(en("Then we can we can deploy the app"), "Then we can deploy the app");
        assert_eq!(clean("I want to I want to fix the login bug", None), "I want to fix the login bug");
    }

    #[test]
    fn keeps_first_copy_of_stuttered_word() {
        assert_eq!(en("We we should test it"), "We should test it");
        assert_eq!(en("I I I think so"), "I think so");
    }

    #[test]
    fn keeps_deliberate_repeats() {
        for s in [
            "Hello, hello, hello, is this on?",
            "I know that that works",
            "a long long time ago",
            "call 5 5 5 now",
            "dial one one two",
            "uh-huh, that sounds right",
        ] {
            assert_eq!(en(s), s);
        }
    }

    #[test]
    fn leaves_other_languages_and_empties_filler_only_text() {
        assert_eq!(clean(" Wir treffen uns um acht Uhr ", Some("de")), "Wir treffen uns um acht Uhr");
        assert_eq!(en("uh, um... hmm"), "");
        assert_eq!(clean("Uh, um.", None), "");
        assert_eq!(clean("Wir treffen uns um acht Uhr", None), "Wir treffen uns um acht Uhr");
        assert_eq!(en(""), "");
    }

    #[test]
    fn english_detection_follows_the_chosen_language() {
        assert!(is_english("Bonjour tout le monde", Some("en-US")));
        assert!(!is_english("Hello there", Some("fr")));
        assert!(!is_english("Bonjour à tous, je voudrais réserver une table pour ce soir.", None));
    }
}
