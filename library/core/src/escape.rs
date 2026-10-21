//! Helper code for character escaping.

use safety::{ensures, requires};

use crate::ascii;
use crate::fmt::{self, Write};
#[cfg(kani)]
use crate::kani;
use crate::marker::PhantomData;
use crate::num::NonZero;
use crate::ops::Range;

const HEX_DIGITS: [ascii::Char; 16] = *b"0123456789abcdef".as_ascii().unwrap();

/// Escapes a character with `\x` representation.
///
/// Returns a buffer with the escaped representation and its corresponding range.
#[inline]
const fn backslash<const N: usize>(a: ascii::Char) -> ([ascii::Char; N], Range<u8>) {
    const { assert!(N >= 2) };

    let mut output = [ascii::Char::Null; N];

    output[0] = ascii::Char::ReverseSolidus;
    output[1] = a;

    (output, 0..2)
}

/// Escapes a character with `\xNN` representation.
///
/// Returns a buffer with the escaped representation and its corresponding range.
#[inline]
const fn hex_escape<const N: usize>(byte: u8) -> ([ascii::Char; N], Range<u8>) {
    const { assert!(N >= 4) };

    let mut output = [ascii::Char::Null; N];

    let hi = HEX_DIGITS[(byte >> 4) as usize];
    let lo = HEX_DIGITS[(byte & 0xf) as usize];

    output[0] = ascii::Char::ReverseSolidus;
    output[1] = ascii::Char::SmallX;
    output[2] = hi;
    output[3] = lo;

    (output, 0..4)
}

/// Returns a buffer with the verbatim character and its corresponding range.
#[inline]
const fn verbatim<const N: usize>(a: ascii::Char) -> ([ascii::Char; N], Range<u8>) {
    const { assert!(N >= 1) };

    let mut output = [ascii::Char::Null; N];

    output[0] = a;

    (output, 0..1)
}

/// Escapes an ASCII character.
///
/// Returns a buffer with the escaped representation and its corresponding range.
const fn escape_ascii<const N: usize>(byte: u8) -> ([ascii::Char; N], Range<u8>) {
    const { assert!(N >= 4) };

    #[cfg(feature = "optimize_for_size")]
    {
        match byte {
            b'\t' => backslash(ascii::Char::SmallT),
            b'\r' => backslash(ascii::Char::SmallR),
            b'\n' => backslash(ascii::Char::SmallN),
            b'\\' => backslash(ascii::Char::ReverseSolidus),
            b'\'' => backslash(ascii::Char::Apostrophe),
            b'"' => backslash(ascii::Char::QuotationMark),
            0x00..=0x1F | 0x7F => hex_escape(byte),
            _ => match ascii::Char::from_u8(byte) {
                Some(a) => verbatim(a),
                None => hex_escape(byte),
            },
        }
    }

    #[cfg(not(feature = "optimize_for_size"))]
    {
        /// Lookup table helps us determine how to display character.
        ///
        /// Since ASCII characters will always be 7 bits, we can exploit this to store the 8th bit to
        /// indicate whether the result is escaped or unescaped.
        ///
        /// We additionally use 0x80 (escaped NUL character) to indicate hex-escaped bytes, since
        /// escaped NUL will not occur.
        const LOOKUP: [u8; 256] = {
            let mut arr = [0; 256];
            let mut idx = 0;
            while idx <= 255 {
                arr[idx] = match idx as u8 {
                    // use 8th bit to indicate escaped
                    b'\t' => 0x80 | b't',
                    b'\r' => 0x80 | b'r',
                    b'\n' => 0x80 | b'n',
                    b'\\' => 0x80 | b'\\',
                    b'\'' => 0x80 | b'\'',
                    b'"' => 0x80 | b'"',

                    // use NUL to indicate hex-escaped
                    0x00..=0x1F | 0x7F..=0xFF => 0x80 | b'\0',

                    idx => idx,
                };
                idx += 1;
            }
            arr
        };

        let lookup = LOOKUP[byte as usize];

        // 8th bit indicates escape
        let lookup_escaped = lookup & 0x80 != 0;

        // SAFETY: We explicitly mask out the eighth bit to get a 7-bit ASCII character.
        let lookup_ascii = unsafe { ascii::Char::from_u8_unchecked(lookup & 0x7F) };

        if lookup_escaped {
            // NUL indicates hex-escaped
            if matches!(lookup_ascii, ascii::Char::Null) {
                hex_escape(byte)
            } else {
                backslash(lookup_ascii)
            }
        } else {
            verbatim(lookup_ascii)
        }
    }
}

/// Escapes a character with `\u{NNNN}` representation.
///
/// Returns a buffer with the escaped representation and its corresponding range.
const fn escape_unicode<const N: usize>(c: char) -> ([ascii::Char; N], Range<u8>) {
    const { assert!(N >= 10 && N < u8::MAX as usize) };

    let c = c as u32;

    // OR-ing `1` ensures that for `c == 0` the code computes that
    // one digit should be printed.
    let start = (c | 1).leading_zeros() as usize / 4 - 2;

    let mut output = [ascii::Char::Null; N];
    output[3] = HEX_DIGITS[((c >> 20) & 15) as usize];
    output[4] = HEX_DIGITS[((c >> 16) & 15) as usize];
    output[5] = HEX_DIGITS[((c >> 12) & 15) as usize];
    output[6] = HEX_DIGITS[((c >> 8) & 15) as usize];
    output[7] = HEX_DIGITS[((c >> 4) & 15) as usize];
    output[8] = HEX_DIGITS[((c >> 0) & 15) as usize];
    output[9] = ascii::Char::RightCurlyBracket;
    output[start + 0] = ascii::Char::ReverseSolidus;
    output[start + 1] = ascii::Char::SmallU;
    output[start + 2] = ascii::Char::LeftCurlyBracket;

    (output, (start as u8)..(N as u8))
}

/// Checks the precondition of [`EscapeIterInner::new`] and
/// [`EscapeIterInner::to_str_unchecked`]: `alive` lies within the buffer, and
/// every byte in it is printable ASCII.
#[cfg(kani)]
fn escape_seq_is_printable<const N: usize>(
    data: &MaybeEscapedCharacter<N>,
    alive: &Range<u8>,
) -> bool {
    if alive.start > alive.end || usize::from(alive.end) > N {
        return false;
    }
    let bytes = escape_seq_bytes(data, alive);
    let mut i = 0;
    while i < bytes.len() {
        if !is_printable(bytes[i]) {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg(kani)]
fn is_printable(b: u8) -> bool {
    matches!(b, 0x20..=0x7E)
}

/// Returns the bytes of the escape in `alive`. Indexing is bounds-checked, so a
/// bad range fails the proof instead of reading out of bounds.
#[cfg(kani)]
fn escape_seq_bytes<'a, const N: usize>(
    data: &'a MaybeEscapedCharacter<N>,
    alive: &Range<u8>,
) -> &'a [u8] {
    // SAFETY: the caller guarantees data holds an escape, so all N bytes are
    // initialized. Reading them as u8 works even if a byte is not valid ASCII.
    let bytes = unsafe { &*(&raw const *data).cast::<[u8; N]>() };
    &bytes[usize::from(alive.start)..usize::from(alive.end)]
}

#[derive(Clone, Copy)]
union MaybeEscapedCharacter<const N: usize> {
    pub escape_seq: [ascii::Char; N],
    pub literal: char,
}

/// Marker type to indicate that the character is always escaped,
/// used to optimize the iterator implementation.
#[derive(Clone, Copy)]
pub(crate) struct AlwaysEscaped;

/// Marker type to indicate that the character may be escaped,
/// used to optimize the iterator implementation.
#[derive(Clone, Copy)]
pub(crate) struct MaybeEscaped;

/// An iterator over a possibly escaped character.
#[derive(Clone)]
pub(crate) struct EscapeIterInner<const N: usize, ESCAPING> {
    // Invariant:
    //
    // If `alive.end <= Self::LITERAL_ESCAPE_START`, `data` must contain
    // printable ASCII characters in the `alive` range of its `escape_seq` variant.
    //
    // If `alive.end > Self::LITERAL_ESCAPE_START`, `data` must contain a
    // `char` in its `literal` variant, and the `alive` range must have a
    // length of at most `1`.
    data: MaybeEscapedCharacter<N>,
    alive: Range<u8>,
    escaping: PhantomData<ESCAPING>,
}

impl<const N: usize, ESCAPING> EscapeIterInner<N, ESCAPING> {
    const LITERAL_ESCAPE_START: u8 = 128;

    /// # Safety
    ///
    /// `data.escape_seq` must contain an escape sequence in the range given by `alive`.
    #[inline]
    #[requires(escape_seq_is_printable(&data, &alive))]
    #[ensures(|result: &Self| result.alive == old(alive.clone())
        // SAFETY: the precondition guarantees data holds an escape.
        && unsafe { result.data.escape_seq } == old(unsafe { data.escape_seq }))]
    const unsafe fn new(data: MaybeEscapedCharacter<N>, alive: Range<u8>) -> Self {
        // Longer escape sequences are not useful given `alive.end` is at most
        // `Self::LITERAL_ESCAPE_START`.
        const { assert!(N < Self::LITERAL_ESCAPE_START as usize) };

        // Check bounds, which implicitly also checks the invariant
        // `alive.end <= Self::LITERAL_ESCAPE_START`.
        debug_assert!(alive.end <= (N + 1) as u8);

        Self { data, alive, escaping: PhantomData }
    }

    pub(crate) const fn backslash(c: ascii::Char) -> Self {
        let (escape_seq, alive) = backslash(c);
        // SAFETY: `escape_seq` contains an escape sequence in the range given by `alive`.
        unsafe { Self::new(MaybeEscapedCharacter { escape_seq }, alive) }
    }

    pub(crate) const fn ascii(c: u8) -> Self {
        let (escape_seq, alive) = escape_ascii(c);
        // SAFETY: `escape_seq` contains an escape sequence in the range given by `alive`.
        unsafe { Self::new(MaybeEscapedCharacter { escape_seq }, alive) }
    }

    pub(crate) const fn unicode(c: char) -> Self {
        let (escape_seq, alive) = escape_unicode(c);
        // SAFETY: `escape_seq` contains an escape sequence in the range given by `alive`.
        unsafe { Self::new(MaybeEscapedCharacter { escape_seq }, alive) }
    }

    #[inline]
    pub(crate) const fn empty() -> Self {
        // SAFETY: `0..0` ensures an empty escape sequence.
        unsafe { Self::new(MaybeEscapedCharacter { escape_seq: [ascii::Char::Null; N] }, 0..0) }
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        usize::from(self.alive.end - self.alive.start)
    }

    #[inline]
    pub(crate) fn advance_by(&mut self, n: usize) -> Result<(), NonZero<usize>> {
        self.alive.advance_by(n)
    }

    #[inline]
    pub(crate) fn advance_back_by(&mut self, n: usize) -> Result<(), NonZero<usize>> {
        self.alive.advance_back_by(n)
    }

    /// Returns a `char` if `self.data` contains one in its `literal` variant.
    #[inline]
    const fn to_char(&self) -> Option<char> {
        if self.alive.end > Self::LITERAL_ESCAPE_START {
            // SAFETY: We just checked that `self.data` contains a `char` in
            //         its `literal` variant.
            return Some(unsafe { self.data.literal });
        }

        None
    }

    /// Returns the printable ASCII characters in the `escape_seq` variant of `self.data`
    /// as a string.
    ///
    /// # Safety
    ///
    /// - `self.data` must contain printable ASCII characters in its `escape_seq` variant.
    /// - `self.alive` must be a valid range for `self.data.escape_seq`.
    #[inline]
    #[requires(escape_seq_is_printable(&self.data, &self.alive))]
    #[ensures(|result: &&str| result.as_bytes() == escape_seq_bytes(&self.data, &self.alive))]
    unsafe fn to_str_unchecked(&self) -> &str {
        debug_assert!(self.alive.end <= Self::LITERAL_ESCAPE_START);

        // SAFETY: The caller guarantees `self.data` contains printable ASCII
        //         characters in its `escape_seq` variant, and `self.alive` is
        //         a valid range for `self.data.escape_seq`.
        unsafe {
            self.data
                .escape_seq
                .get_unchecked(usize::from(self.alive.start)..usize::from(self.alive.end))
                .as_str()
        }
    }
}

impl<const N: usize> EscapeIterInner<N, AlwaysEscaped> {
    pub(crate) fn next(&mut self) -> Option<u8> {
        let i = self.alive.next()?;

        // SAFETY: The `AlwaysEscaped` marker guarantees that `self.data`
        //         contains printable ASCII characters in its `escape_seq`
        //         variant, and `i` is guaranteed to be a valid index for
        //         `self.data.escape_seq`.
        unsafe { Some(self.data.escape_seq.get_unchecked(usize::from(i)).to_u8()) }
    }

    pub(crate) fn next_back(&mut self) -> Option<u8> {
        let i = self.alive.next_back()?;

        // SAFETY: The `AlwaysEscaped` marker guarantees that `self.data`
        //         contains printable ASCII characters in its `escape_seq`
        //         variant, and `i` is guaranteed to be a valid index for
        //         `self.data.escape_seq`.
        unsafe { Some(self.data.escape_seq.get_unchecked(usize::from(i)).to_u8()) }
    }
}

impl<const N: usize> EscapeIterInner<N, MaybeEscaped> {
    // This is the only way to create any `EscapeIterInner` containing a `char` in
    // the `literal` variant of its `self.data`, meaning the `AlwaysEscaped` marker
    // guarantees that `self.data` contains printable ASCII characters in its
    // `escape_seq` variant.
    pub(crate) const fn printable(c: char) -> Self {
        Self {
            data: MaybeEscapedCharacter { literal: c },
            // Uphold the invariant `alive.end > Self::LITERAL_ESCAPE_START`, and ensure
            // `len` behaves correctly for iterating through one character literal.
            alive: Self::LITERAL_ESCAPE_START..(Self::LITERAL_ESCAPE_START + 1),
            escaping: PhantomData,
        }
    }

    pub(crate) fn next(&mut self) -> Option<char> {
        let i = self.alive.next()?;

        if let Some(c) = self.to_char() {
            return Some(c);
        }

        // SAFETY: At this point, `self.data` must contain printable ASCII
        //         characters in its `escape_seq` variant, and `i` is
        //         guaranteed to be a valid index for `self.data.escape_seq`.
        Some(char::from(unsafe { self.data.escape_seq.get_unchecked(usize::from(i)).to_u8() }))
    }
}

impl<const N: usize> fmt::Display for EscapeIterInner<N, AlwaysEscaped> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: The `AlwaysEscaped` marker guarantees that `self.data`
        //         contains printable ASCII chars, and `self.alive` is
        //         guaranteed to be a valid range for `self.data`.
        f.write_str(unsafe { self.to_str_unchecked() })
    }
}

impl<const N: usize> fmt::Display for EscapeIterInner<N, MaybeEscaped> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(c) = self.to_char() {
            return f.write_char(c);
        }

        // SAFETY: At this point, `self.data` must contain printable ASCII
        //         characters in its `escape_seq` variant, and `self.alive`
        //         is guaranteed to be a valid range for `self.data`.
        f.write_str(unsafe { self.to_str_unchecked() })
    }
}

impl<const N: usize> fmt::Debug for EscapeIterInner<N, AlwaysEscaped> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EscapeIterInner").field(&format_args!("'{}'", self)).finish()
    }
}

impl<const N: usize> fmt::Debug for EscapeIterInner<N, MaybeEscaped> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EscapeIterInner").field(&format_args!("'{}'", self)).finish()
    }
}

#[cfg(kani)]
#[unstable(feature = "kani", issue = "none")]
mod verify {
    use super::*;

    // The invariant documented on EscapeIterInner: the range matches what the
    // buffer holds.
    fn is_safe<const N: usize, E>(it: &EscapeIterInner<N, E>) -> bool {
        if it.alive.end > EscapeIterInner::<N, E>::LITERAL_ESCAPE_START {
            // SAFETY: MaybeEscapedCharacter is at least as large and aligned as u32.
            // Reading the bits as u32 rather than char lets an invalid char fail
            // the check instead of being UB.
            let bits = unsafe { (&raw const it.data).cast::<u32>().read() };
            char::from_u32(bits).is_some()
                && it.alive.start <= it.alive.end
                && it.alive.end - it.alive.start <= 1
        } else {
            escape_seq_is_printable(&it.data, &it.alive)
        }
    }

    // Byte i of the escape, read with bounds checks.
    fn escape_byte<const N: usize, E>(it: &EscapeIterInner<N, E>, i: u8) -> u8 {
        // SAFETY: callers only use this while the buffer holds an escape.
        let seq = unsafe { it.data.escape_seq };
        seq[usize::from(i)].to_u8()
    }

    // A fmt::Write that records what Display writes, so the proof can compare it.
    struct Sink<const N: usize> {
        buf: [u8; N],
        len: usize,
        literal: Option<char>,
    }

    impl<const N: usize> fmt::Write for Sink<N> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let bytes = s.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                assert!(self.len < N);
                self.buf[self.len] = bytes[i];
                self.len += 1;
                i += 1;
            }
            Ok(())
        }

        fn write_char(&mut self, c: char) -> fmt::Result {
            assert!(self.literal.is_none() && self.len == 0);
            self.literal = Some(c);
            Ok(())
        }
    }

    // Display writes exactly what is left of the escape. An unescaped char is
    // written even after it has been yielded, as std does.
    fn assert_display_correctness<const N: usize, E>(it: &EscapeIterInner<N, E>)
    where
        EscapeIterInner<N, E>: fmt::Display,
    {
        let mut sink = Sink { buf: [0; N], len: 0, literal: None };
        let mut f = fmt::Formatter::new(&mut sink, fmt::FormattingOptions::new());
        assert!(fmt::Display::fmt(it, &mut f).is_ok());
        if let Some(c) = it.to_char() {
            assert!(sink.literal == Some(c));
        } else {
            assert!(sink.literal.is_none() && sink.len == it.len());
            let mut i = 0;
            while i < sink.len {
                assert!(sink.buf[i] == escape_byte(it, it.alive.start + i as u8));
                i += 1;
            }
        }
    }

    // advance_by(n) skips min(n, len) items and returns how many it could not skip.
    fn assert_advance_correctness(
        before: usize,
        n: usize,
        after: usize,
        result: Result<(), NonZero<usize>>,
    ) {
        let skipped = n.min(before);
        assert!(after == before - skipped);
        assert!(result == NonZero::new(n - skipped).map_or(Ok(()), Err));
    }

    // Runs N + 1 arbitrary operations: N can use up the longest escape, and one more
    // runs on the empty iterator. Every result is checked, and so is the invariant
    // after each step.
    fn exercise_always_escaped<const N: usize>(mut it: EscapeIterInner<N, AlwaysEscaped>) {
        assert!(is_safe(&it) && it.to_char().is_none());
        let mut step = 0;
        while step < N + 1 {
            let before = it.len();
            match kani::any_where(|op: &u8| *op < 5) {
                0 => {
                    let front = it.alive.start;
                    match it.next() {
                        Some(b) => assert!(is_printable(b) && b == escape_byte(&it, front)),
                        None => assert!(before == 0),
                    }
                }
                // next_back has already moved alive.end down to the index it yielded.
                1 => match it.next_back() {
                    Some(b) => assert!(is_printable(b) && b == escape_byte(&it, it.alive.end)),
                    None => assert!(before == 0),
                },
                2 => {
                    let n = kani::any();
                    let result = it.advance_by(n);
                    assert_advance_correctness(before, n, it.len(), result);
                }
                3 => {
                    let n = kani::any();
                    let result = it.advance_back_by(n);
                    assert_advance_correctness(before, n, it.len(), result);
                }
                _ => {
                    assert_display_correctness(&it);
                    kani::cover(before == 0, "Display on an exhausted iterator");
                }
            }
            assert!(is_safe(&it) && it.to_char().is_none());
            step += 1;
        }
    }

    // The same for MaybeEscaped, without next_back and advance_back_by, which
    // char::EscapeDebug never calls.
    fn exercise_maybe_escaped<const N: usize>(mut it: EscapeIterInner<N, MaybeEscaped>) {
        assert!(is_safe(&it));
        let literal = it.to_char();
        let mut step = 0;
        while step < N + 1 {
            let before = it.len();
            match kani::any_where(|op: &u8| *op < 3) {
                0 => {
                    let front = it.alive.start;
                    match (it.next(), literal) {
                        (Some(c), Some(l)) => assert!(c == l),
                        (Some(c), None) => {
                            assert!(c.is_ascii() && is_printable(c as u8));
                            assert!(c as u8 == escape_byte(&it, front));
                        }
                        (None, _) => assert!(before == 0),
                    }
                }
                1 => {
                    let n = kani::any();
                    let result = it.advance_by(n);
                    assert_advance_correctness(before, n, it.len(), result);
                }
                _ => {
                    assert_display_correctness(&it);
                    kani::cover(before == 0, "Display on an exhausted iterator");
                }
            }
            assert!(is_safe(&it));
            assert!(it.to_char() == literal);
            step += 1;
        }
    }

    // Any printable ascii::Char, which covers every argument backslash receives.
    fn any_printable() -> ascii::Char {
        ascii::Char::from_u8(kani::any_where(|b: &u8| is_printable(*b))).unwrap()
    }

    // The iterator holds an escape, not a char, and yields exactly the expected bytes.
    fn assert_sequence<const N: usize, E>(it: &EscapeIterInner<N, E>, expected: &[u8]) {
        assert!(it.to_char().is_none() && it.len() == expected.len());
        let mut i = 0;
        while i < expected.len() {
            assert!(escape_byte(it, it.alive.start + i as u8) == expected[i]);
            i += 1;
        }
    }

    fn hex_digit(nibble: u32) -> u8 {
        if nibble < 10 { b'0' + nibble as u8 } else { b'a' + (nibble - 10) as u8 }
    }

    // \t, \r, \n, \\, \' and \" are backslash-escaped, other printable
    // bytes are kept, and the rest become \xHH.
    fn assert_ascii_correctness<const N: usize, E>(byte: u8, it: &EscapeIterInner<N, E>) {
        match byte {
            b'\t' => assert_sequence(it, b"\\t"),
            b'\r' => assert_sequence(it, b"\\r"),
            b'\n' => assert_sequence(it, b"\\n"),
            b'\\' | b'\'' | b'"' => assert_sequence(it, &[b'\\', byte]),
            0x20..=0x7E => assert_sequence(it, &[byte]),
            _ => assert_sequence(
                it,
                &[b'\\', b'x', hex_digit(u32::from(byte >> 4)), hex_digit(u32::from(byte & 0xf))],
            ),
        }
    }

    // \u{...} holds the code point in lowercase hex without leading zeros.
    fn assert_unicode_correctness<const N: usize, E>(c: char, it: &EscapeIterInner<N, E>) {
        let value = c as u32;
        let mut digits = 1;
        while digits < 6 && value >> (4 * digits) != 0 {
            digits += 1;
        }
        let mut expected = [0; 10];
        expected[0] = b'\\';
        expected[1] = b'u';
        expected[2] = b'{';
        let mut i = 0;
        while i < digits {
            expected[3 + i] = hex_digit((value >> (4 * (digits - 1 - i))) & 0xf);
            i += 1;
        }
        expected[3 + digits] = b'}';
        assert_sequence(it, &expected[..digits + 4]);
    }

    // Add the proof attributes. exercise_* loops N + 1 = 11 times for N = 10, and
    // Kani needs one unwinding more than a loop's iterations, so 12. Every other
    // loop, including the one in the contracts' precondition, is shorter.
    macro_rules! iteration_proof {
        (fn $name:ident() $body:block) => {
            #[kani::proof]
            #[kani::unwind(12)]
            fn $name() $body
        };
    }

    // ascii::EscapeDefault, behind u8::escape_ascii and [u8]::escape_ascii.
    mod ascii_escape_default {
        use super::*;

        // Every u8 gets the right escape, and any sequence of operations then yields
        // exactly that escape.
        iteration_proof! {
            fn ascii() {
                let byte = kani::any();
                let it = EscapeIterInner::<4, AlwaysEscaped>::ascii(byte);
                assert_ascii_correctness(byte, &it);
                kani::cover(it.len() == 1, "a verbatim byte");
                kani::cover(it.len() == 2, "a backslash escape");
                kani::cover(it.len() == 4, "a hex escape");
                exercise_always_escaped(it);
            }
        }

        // The empty iterator, which [u8]::escape_ascii's Display uses when neither
        // end has a partly used escape.
        iteration_proof! {
            fn empty() {
                let it = EscapeIterInner::<4, AlwaysEscaped>::empty();
                assert_sequence(&it, b"");
                exercise_always_escaped(it);
            }
        }
    }

    // char::EscapeDefault and char::EscapeUnicode.
    mod char_escape_default {
        use super::*;

        // Every u8 gets the right escape, and any sequence of operations then yields
        // exactly that escape.
        iteration_proof! {
            fn ascii() {
                let byte = kani::any();
                let it = EscapeIterInner::<10, AlwaysEscaped>::ascii(byte);
                assert_ascii_correctness(byte, &it);
                kani::cover(it.len() == 1, "a verbatim byte");
                kani::cover(it.len() == 2, "a backslash escape");
                kani::cover(it.len() == 4, "a hex escape");
                exercise_always_escaped(it);
            }
        }

        // Every printable ascii::Char becomes a backslash followed by that character.
        iteration_proof! {
            fn backslash() {
                let c = any_printable();
                let it = EscapeIterInner::<10, AlwaysEscaped>::backslash(c);
                assert_sequence(&it, &[b'\\', c.to_u8()]);
                exercise_always_escaped(it);
            }
        }

        // Every char becomes \u{...}, from the shortest, \u{0}, to the longest,
        // \u{10ffff}.
        iteration_proof! {
            fn unicode() {
                let c = kani::any();
                let it = EscapeIterInner::<10, AlwaysEscaped>::unicode(c);
                assert_unicode_correctness(c, &it);
                kani::cover(it.len() == 5, "the shortest unicode escape");
                kani::cover(it.len() == 10, "the longest unicode escape");
                exercise_always_escaped(it);
            }
        }
    }

    // char::EscapeDebug.
    mod char_escape_debug {
        use super::*;

        // Every char kept unescaped is yielded once, and Display writes it unchanged.
        iteration_proof! {
            fn printable() {
                let c = kani::any();
                let it = EscapeIterInner::<10, MaybeEscaped>::printable(c);
                assert!(it.to_char() == Some(c) && it.len() == 1);
                exercise_maybe_escaped(it);
            }
        }

        // Every printable ascii::Char becomes a backslash followed by that character.
        iteration_proof! {
            fn backslash() {
                let c = any_printable();
                let it = EscapeIterInner::<10, MaybeEscaped>::backslash(c);
                assert_sequence(&it, &[b'\\', c.to_u8()]);
                exercise_maybe_escaped(it);
            }
        }

        // Every char becomes \u{...}.
        iteration_proof! {
            fn unicode() {
                let c = kani::any();
                let it = EscapeIterInner::<10, MaybeEscaped>::unicode(c);
                assert_unicode_correctness(c, &it);
                kani::cover(it.len() == 10, "the longest unicode escape");
                exercise_maybe_escaped(it);
            }
        }
    }

    // Any escape buffer: arbitrary ASCII bytes, printable or not.
    fn any_escape_seq<const N: usize>() -> MaybeEscapedCharacter<N> {
        let bytes: [u8; N] = kani::any();
        kani::assume(bytes.is_ascii());
        MaybeEscapedCharacter { escape_seq: *bytes.as_ascii().unwrap() }
    }

    // The contracts of the two unsafe functions, for a given buffer size.
    macro_rules! contract_proofs {
        ($size:ident, $n:literal) => {
            mod $size {
                use super::*;

                #[kani::proof_for_contract(EscapeIterInner::<$n, AlwaysEscaped>::new)]
                #[kani::unwind(12)]
                fn new() {
                    let data = any_escape_seq::<$n>();
                    // SAFETY: proof_for_contract assumes the precondition holds.
                    let _ = unsafe {
                        EscapeIterInner::<$n, AlwaysEscaped>::new(data, kani::any()..kani::any())
                    };
                    kani::cover(true, "the precondition is satisfiable");
                }

                #[kani::proof_for_contract(EscapeIterInner::<$n, AlwaysEscaped>::to_str_unchecked)]
                #[kani::unwind(12)]
                fn to_str_unchecked() {
                    let it = EscapeIterInner::<$n, AlwaysEscaped> {
                        data: any_escape_seq::<$n>(),
                        alive: kani::any()..kani::any(),
                        escaping: PhantomData,
                    };
                    // SAFETY: proof_for_contract assumes the precondition holds.
                    let _ = unsafe { it.to_str_unchecked() };
                    kani::cover(true, "the precondition is satisfiable");
                }
            }
        };
    }

    // Both buffer sizes the standard library uses.
    contract_proofs!(contracts_4, 4);
    contract_proofs!(contracts_10, 10);
}
