# Challenge XXXX[^challenge_id]: Safety and Correctness of Integer Logarithms

- **Status:** Open
- **Solution:** Not yet submitted.
- **Tracking Issue:** Not yet created.
- **Start date:** TBD
- **Reward:** TBD[^reward]

-------------------

## Goal

Verify the safety and correctness of `ilog10`, `ilog2`, and their checked variants in `core::num`.

## Motivation

These safe APIs rely on unsafe operations internally. Decimal logarithms use arithmetic
calculations followed by unsafe result-bound assertions. Binary logarithms use an intrinsic
that requires a nonzero input. Verify that these requirements hold and that the methods
return correct results.

## Description

| Function | Integer Types | Location |
|---------|---------------|----------|
| `ilog10`, `checked_ilog10`, `ilog2`, `checked_ilog2` | All signed and unsigned integer types | `library/core/src/num/int_macros.rs`, `library/core/src/num/uint_macros.rs` |
| `NonZero<T>::ilog2` | All unsigned integer types | `library/core/src/num/nonzero.rs` |
| Decimal-logarithm helpers and wrappers | Width-specific implementations | `library/core/src/num/imp/int_log10.rs` |

- **Signed:** `i8`, `i16`, `i32`, `i64`, `i128`, `isize`.
- **Unsigned:** `u8`, `u16`, `u32`, `u64`, `u128`, `usize`.

Arbitrary-base `ilog` is excluded: its general calculation contains no direct unsafe
operations, and its unsafe dependencies overlap with the implementations above.

### Assumptions

- Target a 64-bit platform; other pointer-width branches are excluded.
- Existing verified NonZero contracts and documented layout guarantees may be reused.

### Success Criteria

For every input to the listed methods:

- Verify the safety conditions of all unsafe operations, including result-bound assertions and nonzero-input requirements.
- Verify correct, rounded-down logarithms for valid inputs.
- Verify `None` or panic behavior for invalid inputs, as documented.
- Prove the calculations do not overflow or underflow.

All proofs must automatically ensure the absence of the following
[undefined behaviors](https://github.com/rust-lang/reference/blob/142b2ed77d33f37a9973772bd95e6144ed9dce43/src/behavior-considered-undefined.md):

- Invoking undefined behavior through compiler intrinsics.
- Reading from uninitialized memory.
- Mutating immutable bytes.
- Producing an invalid value.

Note: All solutions to verification challenges need to satisfy the criteria established in the
[challenge book](../general-rules.md) in addition to the ones listed above.

[^challenge_id]: The number of the challenge sorted by publication date.
[^reward]: Leave it as TBD when creating a new challenge. This should only be filled by the reward committee.
