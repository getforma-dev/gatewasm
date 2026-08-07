//! Char-safe string helpers.
//!
//! Byte-slicing a `&str` panics when an index is not on a UTF-8 char boundary,
//! and the release profile sets `panic = "abort"`, so one such slice takes down
//! the process serving every tenant. `sanitize_custom_css` shipped that bug;
//! `#![warn(clippy::string_slice)]` then found the same shape in the OAuth
//! token endpoint, where the value came straight from an unauthenticated
//! request body.
//!
//! Uniform-width input hides it: `&s[..12]` is safe for any string of
//! all-2-byte, all-3-byte or all-4-byte characters, because 12 divides by 2, 3
//! and 4. It is *mixed* width that breaks — `"a" + "é".repeat(6)` puts byte 12
//! in the middle of the last `é`.

/// The longest prefix of `s` with at most `max` characters.
///
/// Counts characters, not bytes, so the result is never a partial character
/// and the call cannot panic for any input.
// `char_indices` yields char boundaries by definition — this is the one place
// the slice is the point.
#[allow(clippy::string_slice)]
pub fn truncate_chars(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

/// Mask a secret for logging: the first `keep` characters, then an ellipsis.
///
/// Returns the ellipsis alone when the value is short enough that a prefix
/// would reveal most of it.
pub fn mask_prefix(secret: &str, keep: usize) -> String {
    if secret.chars().count() > keep {
        format!("{}…", truncate_chars(secret, keep))
    } else {
        "…".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_chars_never_panics_on_mixed_width_input() {
        // Each of these panicked as `&s[..s.len().min(12)]`. Mixed width is
        // the case that matters: uniform width happens to land on a boundary
        // because 12 divides by 2, 3 and 4.
        for s in [
            &format!("a{}", "é".repeat(6)),
            &format!("ab{}", "日".repeat(5)),
            &format!("abc{}", "🔑".repeat(3)),
            &format!("abcdefghijk{}", "é"),
            &"🔑".repeat(50),
            &String::new(),
        ] {
            let out = truncate_chars(s, 12);
            assert!(out.chars().count() <= 12);
            assert!(s.starts_with(out));
        }
    }

    #[test]
    fn truncate_chars_keeps_whole_characters() {
        assert_eq!(truncate_chars("abcdefghijklmnop", 12), "abcdefghijkl");
        assert_eq!(truncate_chars("aééééééé", 12), "aééééééé");
        assert_eq!(truncate_chars("ab", 12), "ab");
        // A 4-char cap on 4-byte characters returns 4 characters, not 4 bytes.
        assert_eq!(truncate_chars(&"🔑".repeat(8), 4), "🔑🔑🔑🔑");
    }

    #[test]
    fn mask_prefix_does_not_split_characters() {
        assert_eq!(mask_prefix("abcdefghijklmnop", 8), "abcdefgh…");
        assert_eq!(mask_prefix(&format!("a{}", "é".repeat(9)), 8), "aééééééé…");
        // Short values reveal nothing.
        assert_eq!(mask_prefix("abc", 8), "…");
        assert_eq!(mask_prefix("", 8), "…");
    }
}
