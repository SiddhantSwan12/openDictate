//! Port of BetterWispr `VoiceCommands.swift`.

use once_cell::sync::Lazy;
use regex::Regex;

const EDGE_PUNCTUATION: &[char] = &['.', ',', '!', '?', ';', ':', '…', '"', '“', '”'];
const SENTENCE_ENDS: &[char] = &['.', '!', '?', '\n'];
const NOUN_MARKERS: &[&str] = &["a", "an", "the", "this", "that", "oxford", "serial"];
const REQUESTS: &[&str] = &["add", "insert", "put"];
const APOLOGIES: &[&str] = &["sorry", "oops", "wait"];

const ALWAYS_PUNCTUATION: &[(&[&str], &str)] = &[
    (&["exclamation", "mark"], "!"),
    (&["exclamation", "point"], "!"),
    (&["question", "mark"], "?"),
    (&["full", "stop"], "."),
    (&["semicolon"], ";"),
    (&["comma"], ","),
];
const REQUESTED_PUNCTUATION: &[(&[&str], &str)] =
    &[(&["period"], "."), (&["colon"], ":"), (&["dash"], " –"), (&["hyphen"], "-")];

static SPACE_BEFORE_PUNCT: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]+([,.!?;:])").unwrap());
static DOUBLED_PUNCT: Lazy<Regex> = Lazy::new(|| Regex::new(r"([,;:])[,;:]+").unwrap());
static NEWLINE_SPACES: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]*\n[ \t]*").unwrap());

struct Writer<'a> {
    tokens: &'a [String],
    out: String,
    capitalize_next: bool,
}

impl Writer<'_> {
    fn key(&self, j: isize) -> String {
        if j >= 0 && (j as usize) < self.tokens.len() {
            self.tokens[j as usize].to_lowercase().trim_matches(EDGE_PUNCTUATION).to_string()
        } else {
            String::new()
        }
    }

    fn ends_set_off(&self, j: isize) -> bool {
        j < 0
            || self.tokens[j as usize].chars().last().is_some_and(|c| ",.!?;:…".contains(c))
            || APOLOGIES.contains(&self.key(j).as_str())
    }

    fn matches(&self, words: &[&str], j: isize) -> bool {
        words.iter().enumerate().all(|(k, w)| self.key(j + k as isize) == *w)
    }

    fn trim_trailing(&mut self, characters: &str) {
        while let Some(last) = self.out.chars().last() {
            if last == ' ' || characters.contains(last) {
                self.out.pop();
            } else {
                break;
            }
        }
    }

    fn drop_apology(&mut self) {
        self.trim_trailing(",;:");
        let last = self.out.split(' ').last().unwrap_or("").to_string();
        if APOLOGIES.contains(&last.to_lowercase().trim_matches(EDGE_PUNCTUATION)) {
            let len = self.out.len() - last.len();
            self.out.truncate(len);
        }
    }

    fn append(&mut self, word: &str) {
        let mut word = word.to_string();
        if self.capitalize_next {
            if let Some(first) = word.chars().next().filter(|c| c.is_lowercase()) {
                word = first.to_uppercase().collect::<String>() + &word[first.len_utf8()..];
            }
        }
        self.capitalize_next = false;
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push(' ');
        }
        self.out.push_str(&word);
    }

    fn punctuation(&self, j: isize) -> Option<(&'static str, usize)> {
        let previous = self.key(j - 1);
        let requested = REQUESTS.contains(&previous.as_str())
            || (NOUN_MARKERS.contains(&previous.as_str()) && REQUESTS.contains(&self.key(j - 2).as_str()));
        if !requested && NOUN_MARKERS.contains(&previous.as_str()) {
            return None;
        }
        if let Some((words, symbol)) = ALWAYS_PUNCTUATION.iter().find(|(w, _)| self.matches(w, j)) {
            return Some((symbol, words.len()));
        }
        let is_last = self.tokens[(j as usize + 1).min(self.tokens.len())..]
            .iter()
            .all(|t| t.trim_matches(EDGE_PUNCTUATION).is_empty());
        if let Some((words, symbol)) = REQUESTED_PUNCTUATION.iter().find(|(w, _)| self.matches(w, j)) {
            if requested || (is_last && *words == ["period"]) {
                return Some((symbol, words.len()));
            }
        }
        None
    }
}

/// Applies English spoken punctuation, line breaks, "scratch that" and "at the rate" mentions.
pub fn apply(text: &str) -> String {
    let tokens: Vec<String> = text.split([' ', '\t']).filter(|t| !t.is_empty()).map(String::from).collect();
    let n = tokens.len() as isize;
    let mut w = Writer { tokens: &tokens, out: String::new(), capitalize_next: false };
    let mut i: isize = 0;

    while i < n {
        let k = w.key(i);

        if REQUESTS.contains(&k.as_str()) {
            let target = if NOUN_MARKERS.contains(&w.key(i + 1).as_str()) { i + 2 } else { i + 1 };
            if target < n && w.punctuation(target).is_some() {
                i = target;
                continue;
            }
        }

        if let Some((symbol, length)) = w.punctuation(i) {
            let trim = if symbol == "," { ",;:".to_string() } else { ",;:.!?".to_string() };
            w.trim_trailing(&trim);
            w.out.push_str(symbol);
            w.capitalize_next = matches!(symbol, "." | "!" | "?");
            i += length as isize;
            continue;
        }

        let next = w.key(i + 1);
        if (k == "new" || k == "next")
            && (next == "line" || next == "paragraph")
            && !NOUN_MARKERS.contains(&w.key(i - 1).as_str())
        {
            w.trim_trailing("");
            w.out.push_str(if next == "line" { "\n" } else { "\n\n" });
            w.capitalize_next = true;
            i += 2;
            continue;
        }

        let forced = (k == "scratch" || k == "strike") && next == "that";
        let set_off = ["remove", "delete", "undo", "cancel"].contains(&k.as_str())
            && next == "that"
            && w.ends_set_off(i - 1)
            && (i + 2 == n || w.ends_set_off(i + 1));
        if forced || set_off {
            w.drop_apology();
            w.trim_trailing(",;:.!?…");
            match w.out.rfind(SENTENCE_ENDS) {
                Some(end) => w.out.truncate(end + 1),
                None => w.out.clear(),
            }
            w.capitalize_next = true;
            i += 2;
            continue;
        }

        let raw = tokens[i as usize].as_str();
        let at_the_rate = k == "at" && next == "the" && w.key(i + 2) == "rate";
        let at_sign = k == "at" && next == "sign";
        if at_the_rate || at_sign || raw == "@" {
            let name = i + if raw == "@" { 1 } else if at_sign { 2 } else { 3 };
            let name_key = w.key(name);
            if name < n && name_key != "of" && !name_key.is_empty() {
                w.capitalize_next = false;
                let mention = format!("@{}", tokens[name as usize]);
                w.append(&mention);
                i = name + 1;
                continue;
            }
        }

        w.append(raw);
        i += 1;
    }

    let out = SPACE_BEFORE_PUNCT.replace_all(&w.out, "$1");
    let out = DOUBLED_PUNCT.replace_all(&out, "$1");
    let out = NEWLINE_SPACES.replace_all(&out, "\n");
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::apply;

    #[test]
    fn insert_punctuation_and_backtrack() {
        assert_eq!(apply("hello comma world"), "hello, world");
        assert_eq!(apply("Hello, comma, how are you"), "Hello, how are you");
        assert_eq!(apply("thanks add a comma see you soon"), "thanks, see you soon");
        assert_eq!(apply("is it done question mark"), "is it done?");
        assert_eq!(apply("first new line second"), "first\nSecond");
        assert_eq!(apply("Send it Monday. Sorry, remove that. Send it Tuesday."), "Send it Tuesday.");
        assert_eq!(apply("Let's meet at 5, sorry, remove that, let's meet at 6"), "Let's meet at 6");
        assert_eq!(apply("Hi team. Ship it today scratch that tomorrow"), "Hi team. Tomorrow");
        assert_eq!(apply("Please remove that file"), "Please remove that file");
        assert_eq!(apply("I love the Oxford comma"), "I love the Oxford comma");
        assert_eq!(apply("the period ended"), "the period ended");
    }

    #[test]
    fn at_the_rate_becomes_mentions() {
        assert_eq!(apply("ping at the rate KV about it"), "ping @KV about it");
        assert_eq!(apply("ask at sign Sam"), "ask @Sam");
        assert_eq!(apply("growing at the rate of 5 percent"), "growing at the rate of 5 percent");
    }
}
