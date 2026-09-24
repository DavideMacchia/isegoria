# OTS fixture provenance

Two real OpenTimestamps proofs, copied byte-for-byte from the test vectors of
`opentimestamps` 0.2.0 (`src/lib.rs`, `SMALL_TEST` and `LARGE_TEST`; MIT OR Apache-2.0).
They check that the resource bounds `anchoring` enforces before parsing an untrusted
proof (T44, AT-NET-07) do not reject genuine proofs.

| File | Library constant | Bytes | Content |
|---|---|---|---|
| `pending-two-calendars.ots` | `SMALL_TEST` | 265 | a SHA-256 digest committed to two calendars, both attestations still pending |
| `bitcoin-attested.ots` | `LARGE_TEST` | 1768 | a complete proof down to a Bitcoin block attestation, including the transaction and the block Merkle path |

Do not edit these files by hand.
