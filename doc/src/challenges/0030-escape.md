# Challenge 30: Safety and Correctness of Character Escaping

- **Status:** Open
- **Solution:** Not yet submitted.
- **Tracking Issue:** TBD
- **Reward:** TBD

-------------------

## Goal

Verify the safety and correctness of `core::escape`, which implements `char::escape_debug`,
`char::escape_default`, `char::escape_unicode`, and `u8::escape_ascii`.

## Motivation

Every `{:?}` of a `str` or `char` writes its escapes through this module. The module reads a
union, indexes a buffer, and builds a `&str`, all without checks, so a single mistake in how it
tracks its state would be undefined behavior in one of Rust's most common formatting paths.

## Description

All functions are defined in [escape.rs](https://github.com/model-checking/verify-rust-std/blob/main/library/core/src/escape.rs).

Four public escape iterators wrap one private type, `EscapeIterInner<N, ESCAPING>`:

| Public type | Wraps | Location |
|---|---|---|
| `ascii::EscapeDefault` | `EscapeIterInner<4, AlwaysEscaped>` | `library/core/src/ascii.rs` |
| `char::EscapeDefault`, `char::EscapeUnicode` | `EscapeIterInner<10, AlwaysEscaped>` | `library/core/src/char/mod.rs` |
| `char::EscapeDebug` | `EscapeIterInner<10, MaybeEscaped>` | `library/core/src/char/mod.rs` |

`EscapeIterInner` stores two things:

- **A buffer of `N` bytes holding the escape**, such as `\n`, `\x7f` or `\u{1f600}`. Four bytes fit
  `\xHH`, and ten fit the longest `\u{...}`. `char::EscapeDebug` leaves some characters unescaped,
  such as `a`. For those it stores the `char` itself in the same buffer, so the buffer is a union.
  `MaybeEscaped` marks the type that can do this; `AlwaysEscaped` types only ever hold an escape.
- **A range of buffer positions not yet yielded.** No separate field says whether the buffer holds
  an escape or a `char`. Instead, positions from 128 up mean a `char`, and lower positions mean an
  escape.

The iterator reads the buffer, indexes it, and converts the rest of the escape to `&str`, all
without checks. This is sound only if the range always matches what the buffer holds.

The challenge is to verify:

| Functions | Used by | What to prove |
|---|---|---|
| `new`, `to_str_unchecked` (`unsafe`) | all three | Safety contracts, and that every caller meets them |
| Constructors `ascii`, `backslash`, `unicode`, `empty`, `printable` | the types that call them (see `ascii.rs` and `char/mod.rs`) | They build the correct escape, or keep the `char` unchanged |
| `next`, `next_back`, `advance_by`, `advance_back_by`, `len`, `Display` | `AlwaysEscaped` | They are safe and yield exactly what is left of the escape |
| `next`, `advance_by`, `len`, `Display` | `MaybeEscaped` | The same; `char::EscapeDebug` never calls `next_back` or `advance_back_by` |

[Challenge 1](0001-core-transmutation.md) covers `ascii::Char::from_u8_unchecked` itself; this
challenge covers its call in `escape_ascii`.

### Assumptions

- Only the three instantiations in the first table need to be verified. `EscapeIterInner` is
  private to `core`, so no other code can use a different buffer size or marker.
- Proofs may call the constructors directly with any argument, instead of going through the
  public types. For example, `char::escape_debug` uses Unicode tables to choose between
  `printable`, `backslash` and `unicode`. That choice happens outside this module and has no
  unsafe code. Calling each constructor with every possible argument covers every call the public
  types can make.

### Success Criteria

Cover every argument each constructor can receive: every `u8` for `ascii`, every `char` for
`unicode` and `printable`, and every printable `ascii::Char` for `backslash`.

Prove that:

- The contracts of `new` and `to_str_unchecked` hold, and every caller meets their preconditions.
- Constructors produce the documented escape: `\t`, `\r`, `\n`, `\\`, `\'`, `\"`, `\xHH`, or
  `\u{...}` in lowercase hex without leading zeros. `printable` keeps the `char` unchanged, and
  `empty` holds nothing.
- The invariant documented on `EscapeIterInner` holds after any sequence of operations, including
  once the iterator is exhausted: if the range ends at or below 128, it lies within the buffer and
  covers only printable ASCII; otherwise the buffer holds a valid `char` and the range covers at
  most one position.
- Iteration and `Display` produce exactly what is left of the escape, except that `Display` writes
  an unescaped `char` (`char::EscapeDebug`) even after it has been yielded.
- Every other unsafe operation is sound: union reads, unchecked indexing, and the unchecked `str`
  conversion.

Proofs must run automatically in the repository's CI without breaking existing
verification workflows.

All proofs must automatically check for undefined behavior through compiler
intrinsics, uninitialized reads, and invalid values.

All solutions must also satisfy the [general challenge requirements](../general-rules.md).
