//! Conservative identifier suggestions. Checksums reduce accidental matches;
//! they do not establish that a number is real or that detection is complete.
use super::{Category, Options};
use std::ops::Range;

fn luhn(digits: &[u8]) -> bool {
    (13..=19).contains(&digits.len())
        && digits.iter().any(|d| *d != digits[0])
        && digits
            .iter()
            .rev()
            .enumerate()
            .map(|(i, d)| {
                let n = if i % 2 == 1 { d * 2 } else { *d };
                u32::from(if n > 9 { n - 9 } else { n })
            })
            .sum::<u32>()
            % 10
            == 0
}
pub(super) fn find(text: &str, options: &Options) -> Vec<(Category, Range<usize>)> {
    let b = text.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit()
            || (i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'.'))
        {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_digit() || b" -.".contains(&b[i])) {
            i += 1;
        }
        let end = text[start..i].trim_end_matches([' ', '-', '.']).len() + start;
        let value = &text[start..end];
        if b.get(i).is_some_and(u8::is_ascii_alphabetic) {
            continue;
        }
        let digits: Vec<_> = value
            .bytes()
            .filter(u8::is_ascii_digit)
            .map(|d| d - b'0')
            .collect();
        let category = if options.payment_cards && !value.contains('.') && luhn(&digits) {
            Some(Category::PaymentCard)
        } else if options.us_ssns
            && value.len() == 11
            && value.as_bytes()[3] == b'-'
            && value.as_bytes()[6] == b'-'
            && digits.len() == 9
            && digits[..3] != [0, 0, 0]
            && digits[..3] != [6, 6, 6]
            && digits[0] < 9
            && digits[3..5] != [0, 0]
            && digits[5..] != [0, 0, 0, 0]
        {
            Some(Category::UsSsn)
        } else if options.ip_addresses
            && value.split('.').count() == 4
            && value
                .split('.')
                .all(|p| !p.is_empty() && p.len() <= 3 && p.parse::<u8>().is_ok())
        {
            Some(Category::IpAddress)
        } else {
            None
        };
        if let Some(c) = category {
            result.push((c, start..end));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checksum_and_identifier_boundaries_reduce_false_positives() {
        let o = Options {
            ip_addresses: true,
            ..Default::default()
        };
        for (s, c) in [
            ("4111 1111 1111 1111", Category::PaymentCard),
            ("123-45-6789", Category::UsSsn),
            ("192.168.1.20", Category::IpAddress),
        ] {
            let r = find(s, &o);
            assert_eq!(r.len(), 1, "{s}");
            assert_eq!(r[0].0, c);
            assert_eq!(&s[r[0].1.clone()], s);
        }
        for s in [
            "4111 1111 1111 1112",
            "0000 0000 0000 0000",
            "000-12-3456",
            "666-12-3456",
            "900-12-3456",
            "123-00-3456",
            "123-12-0000",
            "256.1.2.3",
            "2026-11-03",
            "ID123-45-6789",
            "4111111111111111x",
        ] {
            assert!(find(s, &o).is_empty(), "{s}");
        }
        let found = super::super::matches("Card 4111 1111 1111 1111", &o);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Category::PaymentCard);
    }
}
