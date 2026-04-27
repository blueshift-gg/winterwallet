# Winterwallet

`no_std` Winternitz one-time signatures with BIP-39-style keypair derivation. Designed
to verify efficiently on Solana while keeping host-side key management ergonomic.

## ⚠️ Security model — read this first

- **One-time signature scheme.** Signing two different messages with the same
  privkey scalars may weaken security, enabling an attacker to forge a third 
  signature. Please always use [`WinternitzKeypair::sign_and_increment`] to 
  enforce position advancement. Direct `WinternitzPrivkey::sign` consumes the 
  privkey but cannot prevent re-derivation from the same keypair position.
- **Custom derivation.** Hardened-only HMAC-SHA512 with magic string
  `"Winternitz seed"`. **Not** BIP-32 compatible; no Bitcoin/Solana wallet
  will derive matching scalars. BIP-39 seed computation *is* standard
  (verified against Trezor's test vectors).
- **Not formally audited (yet).**

## Layout

| Crate path | Purpose | Compiles on Solana? |
|---|---|---|
| `WinternitzKeypair` | mnemonic-based hierarchical secrets | ✗ host-only |
| `WinternitzPrivkey<N>` | secrets at one position | ✗ host-only |
| `WinternitzSignature<N>` | one-time signature | ✓ |
| `WinternitzPubkey<N>` | derived public key | ✓ |
| `WinternitzRoot` | 32-byte Merkle commitment | ✓ |

`N` is the number of message scalars; must be **even and in `16..=32`**
(enforced at compile time). Total scalars per key/sig is `N + 2`
(message + 2-byte checksum).

## Usage

### Sign

```rust
use winternitz::{WinternitzKeypair, WinternitzSignature};

let mut kp = WinternitzKeypair::from_mnemonic(
    "earn foster affair make exclude object spring oppose one hollow garage kind",
    0, // wallet index
)?;

// Publish this commitment ahead of time.
let root = kp.derive::<32>().to_pubkey().merklize();

// Sign + advance atomically. After this call, `kp` is at the next position.
// Messages are passed as `&[&[u8]]` (matching `solana_sha256_hasher::hashv`),
// so you can mix domain-separation tags or context bytes with the payload
// without an intermediate allocation.
let sig: WinternitzSignature<32> = kp.sign_and_increment(&[b"hello".as_slice()]);
```

### Verify (onchain or off)

```rust
use winternitz::{WinternitzRoot, WinternitzSignature};

// In a Solana program: deserialize sig + root from instruction data zero-copy.
let sig: &WinternitzSignature<32> = sig_bytes.try_into()?;
assert!(sig.verify(&[b"context-tag".as_slice(), payload], root));
```

### Recover a pubkey without a stored root

When initialising a wallet you don't yet have a stored root to verify against —
recover the implied pubkey from the signature and message, then merklize:

```rust
use winternitz::{WinternitzRoot, WinternitzSignature};

let pk = sig.recover_pubkey(&[b"Wallet init".as_slice(), &wallet_id]);
let root: WinternitzRoot = pk.merklize(); // store this for future verifies
```

### Generate a fresh mnemonic

```rust
use winternitz::WinternitzKeypair;

let entropy: [u8; 32] = /* from OsRng or hardware */;
let words = WinternitzKeypair::generate_mnemonic(entropy);
println!("{}", words.join(" "));
```

## Wire format

`Privkey<N>`, `Pubkey<N>`, and `Signature<N>` are `#[repr(C)]` arrays of
`(N+2) * 32` raw bytes (message scalars then checksum scalars). All three have:

- `as_bytes(&self) -> &[u8]` — zero-copy view of the canonical encoding.
- `TryFrom<&'a [u8]> for &'a Self` — zero-copy parse, returns `InvalidLength`
  on size mismatch. Alignment is 1, so casting is sound.

`Display` formats as `0x` + concatenated hex (canonical wire form).
`Debug` (where implemented) shows labelled scalars for inspection.
**`WinternitzPrivkey` deliberately has no `Debug`** to prevent accidental
secret leakage via logging or panic messages.

## Derivation

```
master = HMAC-SHA512("Winternitz seed", bip39_seed)
  └── wallet'                                           ┐
       └── parent'                                      │  4 hardened HMAC levels
            └── child'                                  │
                 ├── 0'  → message scalar 0             ┐
                 ├── 1'  → message scalar 1             │
                 ...                                    │  N+2 hardened leaves
                 ├── N-1'                               │
                 ├── N'  → checksum scalar 0            │
                 └── N+1'→ checksum scalar 1            ┘
```

Position lives on the keypair as `(wallet, parent, child)`.
`increment_child` cascades into `parent` on overflow; `parent` panics on
overflow (4B parents = retire the wallet).

## Compatibility

- `#![no_std]`, no `alloc`.
- Requires a Rust toolchain that supports edition 2024 and inline `const { ... }` blocks.
- On Solana (`target_os = "solana"` / `target_arch = "bpf"`), the
  `Privkey`/`Keypair`/`mnemonic` modules are excluded automatically along with
  their host-only deps (`hmac`, `sha2`, `pbkdf2`, `zeroize`).
- Hashing always goes through `solana-sha256-hasher`, which uses Solana's
  syscall on-chain and the `sha2` crate elsewhere.

## CI

GitHub Actions runs:
- `cargo test` (stable)
- `cargo build --no-default-features` (stable)
- `cargo +nightly fmt --check`
- `cargo +nightly clippy --all-targets -- -D warnings`

## Disclaimer

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE, AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR CONTRIBUTORS BE LIABLE FOR ANY CLAIM, DAMAGES, OR OTHER LIABILITY,
WHETHER IN AN ACTION OF CONTRACT, TORT, OR OTHERWISE, ARISING FROM, OUT OF, OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

Use at your own risk. This code has not been formally audited. You — not the
authors — are responsible for any loss of funds, keys, or data resulting from
its use, misuse, or integration into other systems.
