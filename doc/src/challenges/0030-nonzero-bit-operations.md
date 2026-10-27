# Challenge 30: Safety and Correctness of NonZero Bit Operations

- **Status:** Open
- **Solution:** Not yet submitted.
- **Tracking Issue:** TBD
- **Reward:** TBD

-------------------

## Goal

Verify the safety and correctness of bit operations in `core::num::NonZero`,
including bit scanning and signedness conversions.

## Motivation

These methods use nonzero-only intrinsics, unchecked shifts, and unchecked NonZero
construction. Their safety depends on valid shift counts and results remaining nonzero.

## Description

All methods are defined in [nonzero.rs](https://github.com/model-checking/verify-rust-std/blob/main/library/core/src/num/nonzero.rs).

| Methods | Integer Types | Verification |
|---------|---------------|--------------|
| `leading_zeros`, `trailing_zeros`, `isolate_highest_one`, `isolate_lowest_one` | All signed and unsigned types | Safety and correctness |
| `highest_one`, `lowest_one` | All signed and unsigned types | Correctness of the position returned |
| `bit_width` | All unsigned types | Safety and correctness |
| `cast_signed` | All unsigned types | Safety and preservation of the bit pattern |
| `cast_unsigned` | All signed types | Safety and preservation of the bit pattern |

- **Signed:** `i8`, `i16`, `i32`, `i64`, `i128`, `isize`.
- **Unsigned:** `u8`, `u16`, `u32`, `u64`, `u128`, `usize`.

These methods extend the scope of [Challenge 12](0012-nonzero.md). Constructor
verification and methods already listed there are not repeated.

### Assumptions

- Existing verified NonZero constructor contracts and documented layout guarantees may be reused.

### Success Criteria

Cover every valid NonZero input for each listed integer type. Interpret signed inputs
using their fixed-width binary representation, with bit positions numbered from zero
at the least significant bit.

Prove that:

- `leading_zeros` and `trailing_zeros` return the correct counts.
- `highest_one` and `lowest_one` return the positions of the highest and lowest set bits.
- Isolation methods return exactly the selected set bit.
- `bit_width` returns the highest set-bit position plus one.
- Signedness conversions preserve the bit pattern and produce nonzero results.
- All unsafe preconditions hold, including nonzero intrinsic inputs, valid shift counts, and nonzero constructed results.
- Calculations do not overflow or underflow.

Proofs must run automatically in the repository's CI without breaking existing
verification workflows.

All proofs must automatically check for undefined behavior through compiler
intrinsics, uninitialized reads, and invalid values.

All solutions must also satisfy the [general challenge requirements](../general-rules.md).
