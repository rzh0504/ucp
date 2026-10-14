//! Compiled, case-insensitive substring search.
//!
//! Search uses Unicode default case folding only, not NFC/NFKC (or any other
//! canonical/compatibility normalization). For example, composed `é` and
//! decomposed `e\u{301}` remain distinct. Full case-fold expansions participate
//! in substring matching, so `ss` matches `ß` and `i\u{307}` matches `İ`.

use caseless::Caseless;

/// A trimmed, case-folded query with a precomputed KMP prefix table.
///
/// Construction takes O(m) time and space; each search takes O(n) worst-case
/// time and O(1) additional space, where m and n count folded Unicode scalars.
/// The haystack is folded lazily, never copied into a full folded string.
#[derive(Clone, Debug)]
pub struct SearchQuery {
    needle: Vec<char>,
    prefix: Vec<usize>,
}

impl SearchQuery {
    pub fn new(query: &str) -> Self {
        let needle: Vec<char> = query.trim().chars().default_case_fold().collect();
        let prefix = compile_prefix(&needle, |a, b| a == b);
        Self { needle, prefix }
    }

    /// Returns whether the trimmed query is empty.
    pub fn is_empty(&self) -> bool {
        self.needle.is_empty()
    }

    /// Returns whether the folded query occurs in the folded text.
    ///
    /// An empty query matches every text, including an empty text. Only the
    /// query is trimmed; whitespace inside it and in the text is preserved.
    pub fn matches(&self, text: &str) -> bool {
        self.matches_folded(text.chars().default_case_fold(), |a, b| a == b)
    }

    /// Stops obsolete background searches, including within a single huge entry.
    pub fn matches_while(&self, text: &str, is_current: impl Fn() -> bool) -> bool {
        let mut scanned = 0usize;
        self.matches_folded(
            text.chars().default_case_fold().take_while(|_| {
                scanned += 1;
                scanned % 1024 != 1 || is_current()
            }),
            |a, b| a == b,
        )
    }

    // The comparator lets tests count the actual algorithm's comparisons.
    // Production calls specialize to ordinary character equality, with no
    // counter or per-search allocation.
    fn matches_folded(
        &self,
        text: impl Iterator<Item = char>,
        mut equal: impl FnMut(char, char) -> bool,
    ) -> bool {
        if self.is_empty() {
            return true;
        }

        let mut matched = 0;
        for ch in text {
            loop {
                if equal(ch, self.needle[matched]) {
                    matched += 1;
                    if matched == self.needle.len() {
                        return true;
                    }
                    break;
                }
                if matched == 0 {
                    break;
                }
                matched = self.prefix[matched - 1];
            }
        }
        false
    }
}

fn compile_prefix(needle: &[char], mut equal: impl FnMut(char, char) -> bool) -> Vec<usize> {
    let mut prefix = vec![0; needle.len()];
    let mut matched = 0;
    for index in 1..needle.len() {
        loop {
            if equal(needle[index], needle[matched]) {
                matched += 1;
                break;
            }
            if matched == 0 {
                break;
            }
            matched = prefix[matched - 1];
        }
        prefix[index] = matched;
    }
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(query: &str, text: &str) -> bool {
        let needle: String = query.trim().chars().default_case_fold().collect();
        let haystack: String = text.chars().default_case_fold().collect();
        haystack.contains(&needle)
    }

    #[test]
    fn obsolete_search_stops_inside_a_large_single_entry() {
        let checks = std::cell::Cell::new(0);
        let text = "a".repeat(1_000_000);
        assert!(!SearchQuery::new("missing").matches_while(&text, || {
            checks.set(checks.get() + 1);
            checks.get() < 3
        }));
        assert_eq!(checks.get(), 3);
    }

    #[test]
    fn trims_unicode_whitespace_and_handles_empty_queries() {
        for query in ["", " \t\r\n", "\u{2003}\u{3000}"] {
            let compiled = SearchQuery::new(query);
            assert!(compiled.is_empty());
            assert!(compiled.matches(""));
            assert!(compiled.matches("任意 text 🦀"));
        }

        let compiled = SearchQuery::new("\u{2003} Foo Bar \t");
        assert!(!compiled.is_empty());
        assert!(compiled.matches("prefix FOO BAR suffix"));
        assert!(!compiled.matches("foobar"));
        assert!(!compiled.matches(""));
        assert!(SearchQuery::new(" a  b ").matches("A  B"));
        assert!(!SearchQuery::new(" a  b ").matches("A B"));
    }

    #[test]
    fn uses_full_default_case_folding_on_both_sides() {
        for (query, text) in [
            ("ΟΣ", "ΟΣ"),
            ("ΟΣ", "ος"),
            ("ος", "οσ"),
            ("Σ", "ς"),
            ("Straße", "STRASSE"),
            ("SS", "ß"),
            ("ß", "SS"),
            ("ẞ", "ss"),
            ("İ", "i\u{307}"),
            ("i\u{307}", "İ"),
            ("i", "İ"),
            ("\u{307}", "İ"),
            ("I", "i"),
            ("ﬃ", "FFI"),
            ("ÉCOLE", "école"),
            ("ПРИВЕТ", "привет"),
            ("你好", "🦀你好世界"),
            ("🦀🚀", "你好🦀🚀!"),
            ("\0B", "a\0b"),
        ] {
            assert!(
                SearchQuery::new(query).matches(text),
                "query={query:?}, text={text:?}"
            );
        }
        assert!(!SearchQuery::new("İ").matches("i"));
        assert!(!SearchQuery::new("I").matches("ı"));
        assert!(!SearchQuery::new("🦀🚀").matches("🚀🦀"));
    }

    #[test]
    fn does_not_normalize() {
        for (query, text) in [("é", "e\u{301}"), ("e\u{301}", "é"), ("Ａ", "a")] {
            assert!(!SearchQuery::new(query).matches(text));
        }
    }

    #[test]
    fn handles_prefix_fallback_and_overlaps() {
        let query = SearchQuery::new("ababaca");
        assert_eq!(query.prefix, [0, 0, 1, 2, 3, 0, 1]);
        assert!(query.matches("ababababaca"));
        assert!(!query.matches("abababababa"));
        assert!(SearchQuery::new("aaaaab").matches("aaaaaaaab"));
        assert!(SearchQuery::new("ssx").matches("sßx"));
    }

    #[test]
    fn agrees_with_fully_folded_reference_for_samples_and_generated_text() {
        let mut samples: Vec<String> = [
            "",
            " ",
            "\tΣ\n",
            "ΟΣ",
            "ος",
            "οσ",
            "Straße",
            "SS",
            "ẞ",
            "İ",
            "i\u{307}",
            "ı",
            "é",
            "e\u{301}",
            "ﬃ",
            "你好🦀",
            "ababa",
            "\0",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();

        // Deterministic generation keeps the test reproducible without a
        // random-number dependency. Include expansion, combining, and NUL
        // characters as well as ordinary ASCII and non-Latin characters.
        let alphabet = [
            'a', 'A', 'b', ' ', '\t', 'Σ', 'σ', 'ς', 'ß', 'ẞ', 'İ', 'i', '\u{307}', 'é', 'e',
            '\u{301}', 'ﬃ', '你', '好', '🦀', '\0',
        ];
        let mut seed = 0x1234_5678_u32;
        for index in 0..256 {
            let mut text = String::new();
            for _ in 0..index % 33 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                text.push(alphabet[seed as usize % alphabet.len()]);
            }
            samples.push(text);
        }

        for query in &samples {
            let compiled = SearchQuery::new(query);
            for text in &samples {
                assert_eq!(
                    compiled.matches(text),
                    reference(query, text),
                    "query={query:?}, text={text:?}"
                );
            }
        }
    }

    #[test]
    fn repeated_text_and_long_needles_have_linear_comparison_bounds() {
        for (unit, suffix) in [("a", "b"), ("ß", "x")] {
            let query = format!("{}{suffix}", unit.repeat(4_096));
            let compiled = SearchQuery::new(&query);
            let mut prefix_comparisons = 0;
            let prefix = compile_prefix(&compiled.needle, |a, b| {
                prefix_comparisons += 1;
                a == b
            });
            assert_eq!(prefix, compiled.prefix);
            assert!(prefix_comparisons <= 2 * compiled.needle.len());

            let mut text = unit.repeat(100_000);
            for expected in [false, true] {
                if expected {
                    text.push_str(suffix);
                }
                let mut search_comparisons = 0;
                let found = compiled.matches_folded(text.chars().default_case_fold(), |a, b| {
                    search_comparisons += 1;
                    a == b
                });
                assert_eq!(found, expected);
                assert_eq!(compiled.matches(&text), expected);
                let folded_len = text.chars().default_case_fold().count();
                assert!(
                    search_comparisons <= 2 * folded_len,
                    "{search_comparisons} comparisons for {folded_len} folded scalars"
                );
            }
        }
    }
}
