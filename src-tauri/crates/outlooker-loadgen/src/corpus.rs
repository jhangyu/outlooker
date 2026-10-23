//! Bundled vocabularies (spec Part 04 §12.3): English words with Zipf ranks, Traditional Chinese words joined
//! without spaces, and a 10% mix of both.
use crate::rng::{Rng, Zipf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Zh,
    Mixed,
}

pub struct Corpus {
    pub en: Vec<&'static str>,
    pub zh: Vec<&'static str>,
    en_zipf: Zipf,
    zh_zipf: Zipf,
}

impl Default for Corpus {
    fn default() -> Self {
        Corpus::new()
    }
}

impl Corpus {
    pub fn new() -> Corpus {
        let en: Vec<&'static str> = include_str!("../data/words-en.txt").lines().filter(|l| !l.is_empty()).collect();
        let zh: Vec<&'static str> = include_str!("../data/words-zh.txt").lines().filter(|l| !l.is_empty()).collect();
        let (en_zipf, zh_zipf) = (Zipf::new(en.len()), Zipf::new(zh.len()));
        Corpus { en, zh, en_zipf, zh_zipf }
    }

    /// 60% English, 30% Chinese, 10% mixed.
    pub fn lang(rng: &mut Rng) -> Lang {
        match rng.below(10) {
            0..=5 => Lang::En,
            6..=8 => Lang::Zh,
            _ => Lang::Mixed,
        }
    }

    fn push_word(&self, rng: &mut Rng, lang: Lang, out: &mut String) {
        let zh = match lang {
            Lang::En => false,
            Lang::Zh => true,
            Lang::Mixed => rng.chance(0.5),
        };
        if zh {
            out.push_str(self.zh[self.zh_zipf.sample(rng)]);
        } else {
            if !out.is_empty() && !out.ends_with(['\n', ' ']) {
                out.push(' ');
            }
            out.push_str(self.en[self.en_zipf.sample(rng)]);
            out.push(' ');
        }
    }

    /// Text of about `target_bytes` UTF-8 bytes: sentences of 6–16 words, a paragraph break after 20% of sentences.
    pub fn text(&self, rng: &mut Rng, lang: Lang, target_bytes: usize) -> String {
        let mut out = String::with_capacity(target_bytes + 64);
        while out.len() < target_bytes {
            for _ in 0..rng.range(6, 16) {
                self.push_word(rng, lang, &mut out);
            }
            let trimmed = out.trim_end_matches(' ').len();
            out.truncate(trimmed);
            out.push_str(if lang == Lang::En { ". " } else { "。" });
            if rng.chance(0.2) {
                let trimmed = out.trim_end_matches(' ').len();
                out.truncate(trimmed);
                out.push('\n');
            }
        }
        out.trim_end().to_owned()
    }

    pub fn subject(&self, rng: &mut Rng, lang: Lang) -> String {
        let mut out = String::new();
        for _ in 0..rng.range(2, 6) {
            self.push_word(rng, lang, &mut out);
        }
        out.trim().to_owned()
    }
}
