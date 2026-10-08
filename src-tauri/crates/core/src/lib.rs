//! Platform-independent dictation logic. Each module is a port of the matching
//! BetterWispr Swift file (Apache-2.0, https://github.com/opennookorg/betterwispr).

pub mod cleaner;
pub mod corrections;
pub mod model;
pub mod polish;
pub mod style;
pub mod vocabulary;
pub mod voice_commands;

use whatlang::{Detector, Lang};

/// Languages considered when the user leaves the spoken language on automatic.
/// A short allowlist keeps short English dictations from being misread as a rare language.
const DETECTABLE: &[(Lang, &str)] = &[
    (Lang::Eng, "en"), (Lang::Deu, "de"), (Lang::Fra, "fr"), (Lang::Spa, "es"), (Lang::Ita, "it"),
    (Lang::Por, "pt"), (Lang::Nld, "nl"), (Lang::Pol, "pl"), (Lang::Swe, "sv"), (Lang::Dan, "da"),
    (Lang::Fin, "fi"), (Lang::Ces, "cs"), (Lang::Tur, "tr"), (Lang::Rus, "ru"), (Lang::Ukr, "uk"),
    (Lang::Hin, "hi"), (Lang::Ben, "bn"), (Lang::Mar, "mr"), (Lang::Tam, "ta"), (Lang::Tel, "te"),
    (Lang::Guj, "gu"), (Lang::Urd, "ur"), (Lang::Ara, "ar"), (Lang::Jpn, "ja"), (Lang::Cmn, "zh"),
    (Lang::Kor, "ko"), (Lang::Ind, "id"), (Lang::Vie, "vi"), (Lang::Tha, "th"),
];

/// Detects the dominant language as an ISO 639-1 code; None when the text is empty or undetectable.
pub fn detect_language(text: &str) -> Option<&'static str> {
    if text.trim().is_empty() {
        return None;
    }
    let detector = Detector::with_allowlist(DETECTABLE.iter().map(|(lang, _)| *lang).collect());
    let lang = detector.detect_lang(text)?;
    DETECTABLE.iter().find(|(l, _)| *l == lang).map(|(_, code)| *code)
}
